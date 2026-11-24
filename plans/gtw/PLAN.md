# GTW v1 — Walking and Driving on the Real Earth

**Status:** Draft
**Date:** 2026-11-24
**Related:** [TASKS.md](TASKS.md)

## Goal

Turn the current FableFlight fork into **Grand Theft World v1**: a single-player, free-roam sandbox set on the real Earth. The player spawns on foot anywhere, walks and runs over surface-aware terrain, takes and drives any parked or moving road vehicle, survives crashes and falls, and can enter a parked aircraft at an airport to fly. V1 keeps the existing aircraft and flight systems, but adds no ground weapons, pedestrians, police, interiors, mission content, or multiplayer ground play. The result must hold 60 fps on a mid-range discrete GPU at 1080p and remain identifiable as GTW throughout the player-facing experience.

## Non-goals

- Pedestrians, crowds, or pedestrian AI.
- Police, wanted levels, or pursuit.
- Building interiors, enterable shops, or procedural floors.
- Missions, story, economy, money, fuel purchases, or repair shops.
- Motorcycles, bicycles, boats, trains, or new road-vehicle types beyond the eight existing models.
- Swimming, diving, or underwater traversal.
- Ground weapons or combat in v1. Existing flight-only arcade weapons remain behind the off-by-default arcade toggle.
- Multiplayer ground play. The existing aircraft multiplayer code stays in the tree but is off by default and is not a v1 promise.
- Full world simulation save. V1 saves the player and current vehicle, not every traffic car.
- Per-wheel suspension/tyre simulation. V1 uses single-body arcade physics.
- Traffic-signal AI, lane-change AI, or traffic jams.
- Car cockpits or modeled car interiors.
- Changes to the existing aircraft flight model.

## Current state

The repository is a Rust 2024 / wgpu flight simulator whose application crate is `fablegame` (`Cargo.toml`). The relevant architecture today:

- **One player entity.** `Game` owns exactly one `Plane`, one `aircraft::Spec`, and one airframe model (`src/main.rs:1171-1187`). `Mode` is only `Menu` and `Fly` (`src/main.rs:86-90`). The fixed-step loop advances `self.plane` at 1/180 s with up to 45 substeps (`src/main.rs:5617-5640`).
- **Camera and input are aircraft-specific.** Camera branches read `self.plane`, `self.spec`, and `self.cockpit::Head` directly (`src/main.rs:6003-6189`). `controls()` maps WASD to flight axes (`src/main.rs:3018-3050`). `bind::ACTIONS` is one global flight table with one key per action (`src/bind.rs:84-164`), and `set_key` clears a key from every other action (`src/bind.rs:257-288`).
- **Terrain is real DEM plus satellite imagery.** Terrain tiles are a 65×65 grid built at z14 from Terrarium elevation and imagery (`src/sources.rs:13-15`, `:1096`). `World::ground(lat, lon)` returns only `Ground { h, water }` (`src/world.rs:109-112`, `:329-340`); `World::terrain_ground` samples the most detailed loaded tile (`src/world.rs:546-579`). `World::ground_normal` gives a finite-difference normal (`src/world.rs:342-371`).
- **Surface classes exist only on the GPU.** `ground_mask` rasterizes OSM water, landcover, landuse, road ribbons, and airfield pavement into a 256×256 RGBA mask (`src/sources.rs:978-1094`). The shader reads it (`src/shaders/terrain.wgsl:10`, `:30-42`), but `Tile` keeps only heights, water, and sea (`src/world.rs:16-32`); the mask is uploaded and dropped (`src/world.rs:983-986`). There is no CPU query for asphalt, grass, sand, or pavement.
- **Roads have a real lane graph.** `RoadNet`/`Path` carry centreline points at road-surface height, lanes, lane width, parking offset, speed, class, bridge/tunnel state, and junctions (`src/roads.rs:10-64`). Traffic follows these paths (`src/cars.rs:350-529`). There is no point query for “which road is under this position”, and sidewalks exist only as a 1.9 m width addition in the road ribbon (`src/roads.rs:92-95`, `:642`).
- **Buildings are solid extruded OSM footprints.** `Footprint` has a ring, base, top, and bridge flag (`src/buildings.rs:39-64`). `World::building` returns a `BHit` from the footprint index (`src/world.rs:600-620`). There are no interiors or doors. Trees are solid only through the aircraft path (`src/world.rs:623-664`; `src/plane.rs:2454-2550`).
- **Cars are kinematic traffic.** `src/cars.rs` builds eight procedural models (`N_MODELS = 8`, lengths in `CAR_LEN`) with wheels baked into one mesh (`src/cars.rs:6-9`, `:95-201`). `Car` is a path follower with no mass, tyres, collision, or player entry (`src/cars.rs:269-286`). Parked cars are `Parked` props (`src/roads.rs:952-988`). There is no ground-vehicle physics anywhere.
- **No character exists.** There is no avatar, capsule, pedestrian, or on-foot camera. There is no skeletal animation or GPU skinning; `skin.rs` is for destroyed building facades, and `fgmodel.rs` supports rigid AC3D animations only.
- **Rendering has two relevant paths.** Aircraft use one `DrawObj` per piece with `plane.wgsl`; cars, rocks, airport props, and birds use the instanced `Inst` + `inst.wgsl` path (`src/render.rs:503-562`, `:1002-1061`, `:3359-3363`; `src/shaders/inst.wgsl`). `inst.wgsl` already has vehicle materials (paint, headlight, glass, tail light, trim) and night lighting.
- **Audio is one aircraft synthesiser.** `audio::Params` describes a single aircraft; `Event` is a one-off queue (`src/audio.rs:52-145`). Tyre rolling and braking parameters already exist (`src/audio.rs:96-98`), but there are no positional emitters, traffic engines, or footsteps.
- **Persistence is settings and bindings only.** `Settings::last` stores a flight start (`src/settings.rs:57-91`), `bindings.json` stores controls, and there is no player save. Paths are under `<config>/fablegame/`.
- **Airports already have parked and moving aircraft.** `traffic::Traffic` keeps `parked: Vec<Parked>` with stand, type, and livery (`src/traffic.rs:27-36`, `:121-136`); types include a light single, business jet, regional jet, narrow-body, and wide-body (`src/airmodels.rs:64-70`). Player entry is not implemented.
- **Verification is build/test based.** The repo has `tests/shaders.rs` and `tests/mcp.rs`, no CI workflow, and no `cargo fmt`/`clippy` baseline. `AGENTS.md` requires aircraft work to run `cargo check`, relevant tests, and `cargo test --test shaders`.

## Approach

The chosen design keeps the proven flight path intact and adds ground play as a new layer:

1. **Keep `Plane` and the flight model.** `Game` continues to own `plane`, `spec`, `model`, weapons, gauges, and crash state. A new `plane_visible` flag hides the aircraft until the player takes one.
2. **Add `Mode::Walk` and `Mode::Drive`.** `Game` gains `character: Character`, `vehicle: Option<Vehicle>`, and `last_car: Option<ParkedVehicle>`. The active mode decides which controller, camera, input map, audio source, and HUD are used. World streaming and weather follow `Game::focus_pos()`.
3. **Build one authoritative surface query.** `World::surface_at` combines a retained CPU surface mask (OSM + ESA WorldCover), a per-tile road index built from `RoadNet`, and the existing terrain height. Character movement, vehicle physics, and footstep audio all consume it.
4. **Use single-body arcade vehicle physics.** One body, four visual wheels, a grip curve, an automatic gearbox, ABS/traction control, handbrake drift, and cosmetic roll/pitch. This is the user's chosen simplification over per-wheel raycast physics.
5. **Use a rigid multi-part avatar.** No skeletal skinning. Separate limb meshes posed procedurally and drawn through the existing instanced path, with a clean `Rig` abstraction so a skinned model can replace it later.
6. **Keep traffic kinematic but make it solid and yielding.** Traffic remains a path-follower; it brakes near the player, collides as an oriented box, and can be removed from its tile and turned into a player vehicle.
7. **Enter aircraft only at airports.** Parked airport aircraft are enterable when a compatible built-in type exists; otherwise an airport menu fallback starts a chosen aircraft on the active runway. The existing `start_airport` path is reused.
8. **Save a resume snapshot, not the world.** One versioned JSON autosave records the active mode and the player/vehicle/airplane state, plus clock settings.
9. **Rebrand last.** Package, binary, launcher, menu, config, cache, docs, and attribution move to GTW after the gameplay systems are stable. Upstream auto-update is disabled so it cannot replace the fork.
10. **Add no new crates.** Landcover uses the existing `ureq`/`Fetcher`/`image` stack, save uses existing `serde`, and tests use the existing harness.

Rejected alternatives:

- **Full ECS/entity rewrite.** It would touch every `self.plane` use and make the flight regression surface too large for v1. The mode-plus-side-entities approach keeps the working aircraft path.
- **Skeletal animation now.** It requires a new vertex format, skinning shader, bone palette, animation data, and rigged assets. Rigid parts prove the controller first.
- **Per-wheel physics.** The user chose single-body arcade; per-wheel suspension is deferred.
- **OSM-only surfaces.** It leaves most of the Earth generic. The verified z14 ESA WorldCover PNG endpoint fills the gaps without a GeoTIFF decoder.
- **Depend on Google 3D Tiles for physics.** They are optional, keyed, and limited to a local zone. OSM/DEM remains authoritative.
- **Renaming the crate during feature work.** The rebrand is a separate final phase so compiler errors do not mix with gameplay changes.

## Technical design

### Architecture and data flow

```text
winit events
  -> bind::Context (Foot | Car | Flight)
  -> Game::controls
      -> Mode::Walk  -> Character::step  -> WorldQuery -> avatar Insts -> walk camera -> health HUD
      -> Mode::Drive -> Vehicle::step    -> WorldQuery + traffic bodies -> car Insts -> chase/hood camera -> speed HUD
      -> Mode::Fly   -> Plane::step      -> existing flight path -> aircraft DrawObjs -> existing camera

Game::frame
  -> Game::focus_pos()
  -> World::update(focus)         stream terrain/detail tiles
       -> Sources::build_tile     DEM + imagery + OSM mask + landcover -> Tile.surface
       -> buildings::build        footprints + RoadNet + RoadIndex
  -> World::update_traffic(dt, focus, player_presence)
  -> Game::update_audio(camera, mode)
  -> Game::update_autosave()
  -> Renderer::frame
```

`Game` additions:

```rust
// src/main.rs
enum Mode { Menu, Walk, Drive, Fly }

struct Game {
    // existing fields remain
    mode: Mode,
    character: Character,
    vehicle: Option<Vehicle>,
    last_car: Option<ParkedVehicle>,
    plane_visible: bool,
    surface_probe: Option<(f64, f64)>, // --surface-probe
    // existing plane/spec/model/weapons/gauges/camera/audio remain
}

impl Game {
    fn focus_pos(&self) -> DVec3;      // character, vehicle, or plane
    fn focus_yaw(&self) -> f64;
    fn active_context(&self) -> bind::Context;
    fn surface_at(&self, p: DVec3) -> Option<surface::SurfaceHit>; // adds weather
}
```

`WorldQuery` keeps character and vehicle physics testable without a GPU or streamed world:

```rust
// src/world.rs
pub trait WorldQuery {
    fn surface_at(&self, p: DVec3) -> Option<surface::SurfaceHit>;
    fn building_push_sphere(&self, p: DVec3, radius: f64) -> Option<DVec3>;
    fn building_push_box(&self, p: DVec3, half: [f64; 2], yaw: f64) -> Option<DVec3>;
    fn tree_push_sphere(&self, p: DVec3, radius: f64) -> Option<DVec3>;
    fn tree_push_box(&self, p: DVec3, half: [f64; 2], yaw: f64) -> Option<DVec3>;
}
```

`World` implements the trait. Test worlds (`FlatWorld`, `BlockWorld`, `RoadWorld`) implement it in `#[cfg(test)]`.

### Data model and migrations

#### Surface classes

`src/surface.rs` defines six land classes plus a water sentinel:

```rust
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Surface {
    Asphalt = 0,
    Concrete = 1,
    Dirt = 2,
    Grass = 3,
    Sand = 4,
    Rock = 5,
    Water = 6, // obstacle, not a walkable/drivable class
}

pub struct SurfaceHit {
    pub h: f64,          // metres above the mean sphere
    pub kind: Surface,
    pub water: bool,
    pub road: bool,
    pub sidewalk: bool,
    pub bridge: bool,
}

pub struct SurfaceWeather {
    pub wet: f32,   // 0..1
    pub snow: f32,  // 0..1
    pub ice: f32,   // 0..1
}
```

Mapping table:

| Surface | OSM mask source | WorldCover class | Car grip | Rolling resistance | Foot speed |
|---|---|---|---|---|---|
| Asphalt | `GC_PAVED` from roads, parking, airfield pavement | none; OSM roads win | 1.00 | 0.015 | 1.00 |
| Concrete | road sidewalk band | 50 built-up | 0.95 | 0.018 | 1.00 |
| Dirt | `LC_FARM`, `LC_WET`; unpaved track | 40 cropland, 90 wetland, 95 mangrove | 0.62 | 0.045 | 0.82 |
| Grass | `LC_WOOD`, `LC_SCRUB`, `LC_GRASS`, `GC_PITCH` | 10 tree, 20 shrub, 30 grass, 100 moss | 0.55 | 0.060 | 0.78 |
| Sand | `LC_BARE` | 60 bare/sparse (beach/desert bias) | 0.42 | 0.110 | 0.58 |
| Rock | `LC_ROCK` | 70 snow/ice base; rock from OSM | 0.78 | 0.030 | 0.72 |
| Water | water coverage > 127 | 80 permanent water | blocked | blocked | blocked |

Weather multipliers are applied by `Game::surface_at`: wet asphalt grip ×0.75; snow grip ×0.45 and foot speed ×0.70; ice grip ×0.20 and foot speed ×0.55. Deep water blocks; a shallow band of 8 m from the water edge slows by ×0.65 and is implemented as an approximation from the water mask, not a bathymetric depth.

Surface precedence, highest first:

1. Road index carriageway or sidewalk (exact road height, Asphalt or Concrete).
2. OSM water coverage.
3. OSM ground class (`GC_PAVED`, `GC_PITCH`, `LC_*`).
4. ESA WorldCover class.
5. Default Grass.

#### Retained tile surface mask

`TileData` gains:

```rust
pub surface: Vec<u8>, // MASK * MASK, one Surface per cell
```

`build_tile` derives it from the existing 256×256 RGBA `mask` plus the landcover tile for the z14 ancestor. `Tile` stores it; `World::surface_at` samples it with nearest-cell lookup. A private `World::terrain_sample(lat, lon) -> Option<(Ground, u8)>` shares the existing tile-selection loop with `terrain_ground`, so height and surface class always come from the same loaded tile.

`Sources` gains:

```rust
pub fn landcover(&self, id: TileId) -> Result<Arc<Vec<u8>>>; // z14 only, 256*256
```

The z14 landcover PNG is cached in `Fetcher` under `lc/{z}/{x}/{y}.png`.

#### Road index

`Path` gains:

```rust
pub drive_half: f32,      // carriageway half-width, kerb to kerb
pub walk: bool,           // has a sidewalk band
pub bridge: bool,
pub local: Vec<[f32; 2]>, // tile-unit centreline, y down
pub alt: Vec<f32>,        // absolute surface altitude at each point
```

`src/roads.rs` adds:

```rust
pub struct RoadHit {
    pub path: u32,
    pub s: f32,
    pub dist: f32,
    pub h: f64,
    pub kind: Surface,
    pub sidewalk: bool,
    pub bridge: bool,
    pub hidden: bool, // tunnel
}

pub struct RoadIndex { /* 64 x 64 tile-unit grid of candidate path ids */ }

impl RoadIndex {
    pub fn build(net: &RoadNet, ext: f32) -> Self;
    pub fn query(&self, net: &RoadNet, p: [f32; 2]) -> Option<RoadHit>;
}
```

`BuildingData` gains `road_index: RoadIndex`, built after `roads::build`. `RoadIndex::query` returns the nearest path within `drive_half + if walk { 1.9 } else { 0.0 }`, interpolates `alt`, and classifies the point as Asphalt inside `drive_half` or Concrete in the sidewalk band. A deck more than 2.5 m above the querying body is ignored so a player under a bridge stays on the terrain. Tunnels remain at grade and are not enterable interiors in v1; this limitation is documented.

#### Save schema

`src/save.rs` defines a versioned snapshot:

```rust
#[derive(Serialize, Deserialize)]
pub struct SaveState {
    pub version: u32,
    pub mode: SaveMode,                 // Foot | Drive | Fly
    pub foot: Option<SavedFoot>,
    pub car: Option<SavedCar>,
    pub plane: Option<SavedPlane>,
    pub last_car: Option<SavedCar>,
    pub clock: SavedClock,
}

pub struct SavedFoot { pos: [f64; 3], yaw: f64, health: f32, style: u8 }
pub struct SavedCar { model: u8, colour: [f32; 3], pos: [f64; 3], yaw: f64, speed: f64, health: f32, damage: f32 }
pub struct SavedPlane { spec: String, pos: [f64; 3], rot: [f64; 4], vel: [f64; 3], gear: f32, flaps: f32, throttle: f32, fuel: f32 }
pub struct SavedClock { utc: f64, doy: f64 }
```

The file is `<config>/gtw/save.json`, written through a temporary file and renamed. A corrupt or unsupported version falls back to a new game. The save is a resume snapshot: traffic positions and full weather particles are not saved. Time, day, and snow setting are saved; live weather is re-fetched.

#### Settings additions

`Settings` gains:

```rust
pub ground_first_person: bool, // default false
pub avatar_style: u8,          // 0..3, default 0
pub autosave: bool,            // default true
```

`Settings::default().multiplayer` changes to `false`. The existing multiplayer settings and code remain reachable. Existing settings files missing the new keys use the defaults.

### Interfaces, configuration, and CLI

- `World::surface_at(p: DVec3) -> Option<SurfaceHit>` is the single gameplay query used by character and vehicle physics; it receives the body height and applies the 2.5 m bridge filter. `World::surface(lat, lon)` is a diagnostic wrapper without the height filter. `World::road_hit(lat, lon) -> Option<RoadHit>` is exposed for diagnostics.
- `Character::step(dt, &CharacterControls, &impl WorldQuery, &SurfaceWeather)` returns a `CharacterStep` with position, velocity, grounded state, and footstep events.
- `Vehicle::step(dt, &VehicleControls, &impl WorldQuery, &SurfaceWeather, &[TrafficBody])` returns a `VehicleStep` with pose, wheel transforms, damage events, and collision impulses.
- `Avatar::instances(camera, &Character, &RigPose, &mut Vec<(u32, Inst)>)` writes rigid limb instances.
- `bind::Context { Foot, Car, Flight }`; `Bindings::key_of`, `map_key`, and `set_key` are context-aware. Global actions (camera, HUD, pause, screenshot) remain available in every context.
- `Game::interact()` enters or exits a car or aircraft; `Game::focus_pos()` drives streaming, weather, minimap, and save.
- New CLI flags:
  - `--start foot|car|plane` selects the initial mode. Default is `foot`, or the saved mode when a save exists.
  - `--flat` sets `World::flat = Some(0.0)` for deterministic headless tests and benchmarks.
  - `--surface-probe LAT,LON` prints the elevation, OSM class, WorldCover class, and merged surface class for a point, then exits.
  - `--no-save` disables autosave and save loading for automated runs.
- Landcover endpoint:
  - Item id from the 3° tile containing the z14 tile: `ESA_WorldCover_10m_2021_v200_{N|S}{DDD}{E|W}{DDD}`.
  - Tile URL: `https://planetarycomputer.microsoft.com/api/data/v1/collections/esa-worldcover/items/{item}/tiles/WebMercatorQuad/{z}/{x}/{y}@1x?assets=map&format=png`.
  - The returned PNG is 256×256 `LA`; the L channel is the raw WorldCover class (10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 100).
  - Cache key `lc/{z}/{x}/{y}.png`; `--offline` reads the cache and falls back to OSM when missing.
  - A z14 tile crossing a 3° boundary composes the up to four overlapping items; missing ocean items are treated as water only when OSM or elevation also says water.
- Config and cache roots are `<config>/gtw/` and `<cache>/gtw/` after the rebrand. A one-time migration copies `settings.json`, `bindings.json`, `graphics.json`, `google_maps.json`, and `launcher.json` from `<config>/fablegame/` when the GTW paths do not exist. The tile cache is reused by renaming `<cache>/fablegame` to `<cache>/gtw` when possible; if the rename fails, the old cache is read as a fallback for one run.

### File changes

| Path | Action | Details |
|---|---|---|
| `src/surface.rs` | create | `Surface`, `SurfaceHit`, OSM/WorldCover mapping, grip/rolling/foot-speed tables, weather multipliers, unit tests. |
| `src/landcover.rs` | create | Item-id calculation, Planetary Computer URL, PNG decode to 256×256 class bytes, boundary composition, fixture tests. |
| `src/sources.rs` | modify | Add `Sources::landcover`, `TileData.surface`, landcover fetch/cache, merge in `build_tile`; no new job kind. |
| `src/world.rs` | modify | Store `Tile.surface`; add `World::surface_at`, diagnostic `surface`, `road_hit`, `building_push_sphere`, `building_push_box`, `tree_push_sphere`, `tree_push_box`, `WorldQuery` trait, spawn-safety helper, tests. |
| `src/roads.rs` | modify | Add `Path.drive_half`, `walk`, `bridge`, `local`, `alt`; add `RoadIndex`/`RoadHit`; build and test the index; classify sidewalks. |
| `src/buildings.rs` | modify | Add `BuildingData.road_index`; build it after `roads::build`; tests for the data handoff. |
| `src/character.rs` | create | Character state, controls, capsule movement, step-up, slopes, jump, surface speed, health/regen/respawn, footsteps, tests. |
| `src/avatar.rs` | create | Rigid human parts, material codes, palettes, procedural `RigPose`, instance generation, mesh constants, tests. |
| `src/vehicle.rs` | create | Single-body arcade physics, auto gearbox, grip circle, handbrake drift, surface grip, damage, OBB collisions, `VehicleStep`, tests. |
| `src/save.rs` | create | Versioned save schema, atomic load/save, corrupt fallback, tests. |
| `src/paths.rs` | create | GTW config/cache roots, one-time legacy migration, tests. |
| `src/bind.rs` | modify | `Context`, foot/car actions, context-aware `key_of`/`map_key`/`set_key`, tests. |
| `src/padmap.rs` | modify | Foot and car gamepad layouts; keep the flight layout unchanged; tests. |
| `src/settings.rs` | modify | New settings fields; multiplayer default off; GTW paths via `paths`; migration tests. |
| `src/settings_ui.rs` | modify | Context tabs for controls; avatar style and autosave settings. |
| `src/main.rs` | modify | `Mode::Walk/Drive`, `Game` fields, `focus_pos`, mode-branched controls/camera/HUD/audio, start-on-foot, go-to-ground, interaction, autosave, `--start`/`--flat`/`--surface-probe`/`--no-save`, avatar and car instances, health/speed HUD, plane visibility. |
| `src/cars.rs` | modify | Split player car body and wheel meshes; expose wheel positions and dimensions; keep traffic models unchanged; tests. |
| `src/roads.rs` | modify | Parked-car lookup/removal helpers used by `World`; tests. |
| `src/world.rs` | modify | Traffic/parked car extraction, traffic-body gathering, traffic yield hook, player-proximity update, removal on carjack, tests. |
| `src/traffic.rs` | modify | `take_parked(stand)` and `parked_at` exposure; map light single/business jet to built-in specs; tests. |
| `src/audio.rs` | modify | Positional emitter model, player car/traffic voices, footstep and crash events, nearest-emitter mixing, pan/gain tests. |
| `src/shaders/inst.wgsl` | modify | Character material branch; player-car damage deformation; keep traffic behavior unchanged. |
| `src/shaders/terrain.wgsl` | modify | No visual change required; align comments/classes only if the retained mask constants move. |
| `src/menu.rs` | modify | Continue/New Game, airport menu fallback, ground-start copy. |
| `src/mainmenu.rs` | modify | GTW title strings and continue affordance. |
| `src/i18n.rs` | modify | GTW locale directory; keep existing keys. |
| `src/fetch.rs` | modify | GTW user agent; cache path passed by callers. |
| `src/gmaps.rs` | modify | GTW config/cache paths. |
| `src/mcp/rpc.rs` | modify | GTW config path. |
| `src/copilot.rs` | modify | GTW screenshot path and crate name. |
| `src/autopilot.rs` | modify | Test cache path only. |
| `src/lib.rs` | modify | GTW crate doc and any shared module exports needed by bins. |
| `src/bin/fableflight-mcp.rs` | rename | Rename to `src/bin/gtw-mcp.rs`; update crate imports, help text, MCP server name. |
| `src/bin/fableflight-launcher/` | rename | Rename to `src/bin/gtw-launcher/`; update titles, paths, binary names, update source, and disable automatic upstream updates. |
| `Cargo.toml` | modify | Package `gtw`, default-run `gtw`; keep edition, rust-version, license, dependencies. |
| `tests/gtw_smoke.rs` | create | Headless `--flat` smoke runs for foot and car, plus a plane CLI parse check; update bin env name after rebrand. |
| `tests/fixtures/landcover_z14.png` | create | Small LA PNG with known WorldCover codes for decode tests. |
| `tests/shaders.rs` | modify only if needed | The existing shader harness picks up `inst.wgsl`; no new test file is required. |
| `tests/mcp.rs` | modify | Update `CARGO_BIN_EXE_*` and MCP server name after the rebrand. |
| `README.md` | modify | GTW identity, controls, build/run names, config paths, landcover attribution, non-goals. |
| `AGENTS.md` | modify | GTW project scope and ground-play conventions; keep the aircraft rules for flight work. |
| `docs/` | modify | Update GTW names, config/cache paths, and binary names in `docs/mcp.md`, `docs/translating.md`, `docs/upscaling.md`, and any other page that references FableFlight paths; leave aircraft documentation intact. |
| `assets/aircraft/liveries/stills.sh` | modify | Point the developer script at the renamed `gtw` release binary. |
| `assets/locales/eng.lng` | modify | Foot/car action labels, airport menu, health/speed HUD, GTW strings. |
| `assets/locales/fr.lng` | modify | Same keys in French. |
| `assets/locales/ko.lng` | modify | Same keys in Korean; fall back to English where translation is unavailable. |

### Failure modes and edge cases

- **Missing terrain tile under the player.** Streaming keeps the player tile loaded. If `World::surface_at` returns `None`, character and vehicle physics hold their last valid sample and do not fall; the loading overlay remains until ground is available.
- **Missing landcover.** The landcover fetch is optional. A 404, timeout, offline cache miss, or decode failure falls back to the OSM mask and then Grass. The tile still loads.
- **3° landcover boundary.** A z14 tile crossing a WorldCover boundary composes up to four item tiles. Missing ocean items do not become land.
- **Road tile not loaded.** The terrain mask still supplies a surface; the road index simply refines it when the building tile is ready. A road hit is never required for movement.
- **Bridge deck vs terrain.** Road hits more than 2.5 m above the queried body are ignored. A character under a bridge stays on terrain; a car approaching a bridge picks up the deck as it rises.
- **Tunnels.** Tunnels are at grade and have no interior geometry in v1. They are not enterable; this is documented in the plan and README.
- **Water.** `Surface::Water` blocks walking and driving. A car entering water stalls and takes engine damage; it is not buoyant. No swimming.
- **Streaming eviction while driving.** The active vehicle lives in `Game`, not in a tile, so eviction does not remove it. `last_car` also lives in `Game`.
- **Carjacking a moving car.** The car is removed from its lane and traffic slot before it becomes a player vehicle. If the tile is not ready, the interaction is refused rather than silently losing the car.
- **Parked aircraft mapping.** Only the light single and business jet traffic types map to built-in player specs. Larger parked types are scenery; the airport menu fallback handles them.
- **No aircraft at an airport.** The airport menu fallback offers a runway start. The player is never stranded.
- **Exiting an aircraft in the air.** Refused with a status message. Exit is allowed only on the ground at low speed.
- **Save corruption.** A malformed or future-version save is ignored, a status message is shown, and a new game starts. Writes are atomic.
- **Saved position invalid.** If the saved character/car position is inside a building, in water, or over a missing tile, `Game` searches a nearby safe point before spawning.
- **Audio device missing.** The existing reopen loop remains. Positional emitters are capped and mixed on the synth thread; no panic when the device is absent.
- **Rebrand migration.** If GTW config does not exist and FableFlight config does, copy known files. If both exist, leave GTW untouched. If the cache rename fails, continue with the old cache for the session.
- **Upstream updater.** The launcher's FableFlight update path is disabled. It must not replace GTW binaries with upstream FableFlight releases.

### Compatibility, rollout, security, and performance

- **Compatibility.** Existing FableFlight settings, bindings, graphics, and launcher config migrate once. New binding contexts may leave old custom keys in the flight context; a migration remaps only actions that still exist. Save files are versioned. The flight controls and flight model are unchanged in `Mode::Fly`.
- **Rollout.** This is a fork, not a live service. The plan lands phase by phase with the project runnable at each boundary. No feature flag is required, but `--start`, `--flat`, `--no-save`, and `--surface-probe` provide scripted testing.
- **Security.** No new secrets. The landcover endpoint is public and requires no key. The item id is generated from numeric coordinates and constrained to the expected pattern. The fetcher keeps its 20 s timeout, host backoff, 64 MiB response limit, and 3 GiB cache cap. No user-supplied URLs are added.
- **Licensing.** ESA WorldCover 2021 v200 is CC BY 4.0; the README and in-game credits must attribute ESA WorldCover and Microsoft Planetary Computer. The fork keeps the MIT license and upstream FableFlight credit. No real airline or vehicle branding is introduced.
- **Performance.** Surface masks cost 64 KiB per loaded terrain tile (256×256 bytes). At the default 700-tile cap this is about 45 MiB, acceptable next to existing building tiles. Road indexes are 64×64 candidate lists per z14 tile. Surface queries are O(candidates) with no per-frame allocation; character/vehicle sample 1–4 points per substep. Avatar adds roughly 14 instanced parts; traffic audio is capped at six nearby voices. The 60 fps target is measured, not assumed.
- **Memory.** Retaining the surface mask must not retain the 256×256×4 GPU mask. `TileData.mask` continues to be dropped after upload; only the one-byte surface vec is kept.
- **No new dependencies.** `image` already decodes PNG; `ureq`/`Fetcher` already fetch and cache; `serde` already serializes. A GeoTIFF/COG dependency is explicitly avoided.

## Testing strategy

- **Unit tests (in module):**
  - `src/surface.rs`: every OSM class, every WorldCover code, unknown codes, water precedence, weather grip/foot multipliers.
  - `src/landcover.rs`: item ids for N/S/E/W, 3° boundaries, antimeridian, PNG fixture decode, missing/empty PNG handling.
  - `src/roads.rs`: road index on carriageway, sidewalk, off-ribbon, bridge, tunnel flag, tile-boundary path, closest-point interpolation.
  - `src/character.rs`: flat walk, sprint speed by surface, step-up, slope limit, jump, fall damage, health regen, respawn safe point, no NaN under large `dt`.
  - `src/vehicle.rs`: acceleration, braking, top speed, reverse, auto shift, handbrake yaw, surface grip, slope, airborne state, OBB collision, damage, no NaN under large `dt`.
  - `src/save.rs`: round-trip each mode, corrupt JSON, future version, atomic write, invalid-position fallback.
  - `src/bind.rs`: context isolation, same physical key mapped differently in Foot/Car/Flight, global actions available in all contexts, reset/migration.
  - `src/audio.rs`: pan/gain math, nearest-emitter cap, footstep surface mapping.
  - `src/avatar.rs`: pose finite for idle/walk/run/jump/enter/exit, palette bounds, part count.
- **Integration tests:**
  - `tests/gtw_smoke.rs`: run the built binary headless with `--flat` and `--start foot` and `--start car`, assert exit 0 and no panic; parse `--start plane` with an airport flag.
  - `tests/shaders.rs`: parse, validate, and translate the modified `inst.wgsl` (and any touched shader) for MSL.
- **End-to-end/manual:**
  - Start on foot at a real city and at a rural point; verify surfaces, water, buildings, kerbs, slopes, first/third person.
  - Take a parked car, then a moving car; verify traffic yields, collision, damage, health, HUD, drift, exit/re-enter.
  - Drive to an airport, take a parked aircraft, fly, land, exit, re-enter, and use the airport menu fallback.
  - Quit and relaunch in Foot, Drive, and Fly modes and verify resume.
  - Inspect visuals with screenshots and recordings; listen with headphones.
- **Performance:**
  - Run a dense city with `--start car`, `--bench 30`, `--fps-log`, and `FABLE_GPU_TIMES=1`.
  - Confirm 60 fps average on the mid-range target at 1080p and no regression in the flight benchmark.

Exact commands:

```sh
cargo check --all-targets
cargo test
cargo test --test shaders
cargo build --release --locked
cargo run --release -- --place "Buenos Aires" --start foot
cargo run --release -- --place "Buenos Aires" --start car
cargo run --release -- --airport EZE --start plane
cargo run --release -- --headless --flat --start foot --hold 5
cargo run --release -- --headless --flat --start car --hold 5
cargo run --release -- --help
```

## Phase overview

| Phase | Outcome | Depends on |
|---|---|---|
| 1 | Surface identification and world query | none |
| 2 | Player modes, on-foot character, avatar, and input contexts | Phase 1 |
| 3 | Drivable cars, arcade physics, damage, and HUD | Phase 2 |
| 4 | Solid traffic, carjacking, and ground collisions | Phase 3 |
| 5 | Aircraft access, save, and resume | Phases 2, 3, 4 |
| 6 | Positional audio and ground polish | Phases 2, 3, 4 |
| 7 | GTW rebrand, config migration, and documentation | Phase 6 |
| 8 | Full verification and acceptance | Phase 7 |

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Mode/entity changes regress flight | High | Keep `Plane::step` and the flight camera path intact; gate by `Mode`; run the existing flight tests and `--fly-check` after each phase. |
| `self.plane` assumptions spread across `Game` | High | Add `focus_pos()` and branch at the known call sites; use the compiler and smoke runs; do not make `plane` optional. |
| Surface data quality varies globally | Medium | OSM first, WorldCover fill, grass fallback; `--surface-probe`; cache and offline fallback. |
| Planetary Computer endpoint changes or rate-limits | Medium | Cache locally, respect host backoff, allow OSM-only fallback, document the endpoint as replaceable. |
| Single-body arcade feels flat | Medium | Handbrake drift, cosmetic roll/pitch, surface grip, class-specific tuning, telemetry and manual tuning. |
| Rigid avatar looks stiff | Medium | Correct scale, silhouette, procedural gait, clothing palettes; keep `Rig` abstraction for a future skinned model. |
| Car/building collision tunnels at speed | High | Physics substeps at 1/120 s, OBB/SAT, swept horizontal checks, cap displacement per step. |
| Streaming evicts the active car or plane | High | Active vehicle and `last_car` live in `Game`, not in tiles; save snapshot; safe respawn. |
| Audio workload causes dropouts | Medium | Cap traffic voices at six, mix on the synth thread, reuse the existing synth; device reopen loop remains. |
| Rebrand breaks bins, tests, or updater | High | Dedicated final phase; mechanical rename; `cargo check --all-targets`; disable upstream updater; update `CARGO_BIN_EXE_*`. |
| 60 fps target missed in dense cities | High | Measure with `--bench`; keep surface query allocation-free; use existing graphics presets; reduce avatar/traffic audio voices first. |

## Acceptance criteria

- [ ] Starting at any real-world point with `--start foot` spawns the character on the ground, with no aircraft visible at the spawn point.
- [ ] Third-person and first-person walking, running, sprinting, jumping, kerb step-up, slope limits, and deep-water blocking work.
- [ ] Six surface classes change grip, acceleration, braking, rolling resistance, foot speed, and audio; rain, snow, and ice modify them.
- [ ] All eight existing road-vehicle models can be taken while parked or moving, driven, drifted, damaged, exited, and re-entered.
- [ ] Traffic brakes for the player, collides, yields when taken, and resumes afterward.
- [ ] Vehicle damage is persistent for the car's life; health regenerates; death respawns on foot nearby; the vehicle stays where it was.
- [ ] The HUD shows speed, gear, minimap, and health as specified, and hides flight-only instruments on foot or in a car.
- [ ] A parked airport aircraft can be entered and flown; the airport menu fallback works when none is available.
- [ ] The full loop works: spawn on foot, walk, take a car, drive to an airport, take a plane, fly, land, exit, quit, relaunch, and resume in the same mode and place.
- [ ] No pedestrians, police, building interiors, swimming, motorcycles, boats, ground weapons, or multiplayer ground play exist in v1.
- [ ] Arcade weapons remain off by default and flight-only; ground weapons are not implemented.
- [ ] Positional audio covers the player car, nearby traffic, tyres by surface, crashes, and footsteps.
- [ ] The window, launcher, menus, binaries, config paths, README, and credits identify GTW; MIT and upstream FableFlight credit remain; ESA WorldCover attribution is present.
- [ ] The launcher cannot overwrite GTW with an upstream FableFlight release.
- [ ] `cargo check --all-targets`, `cargo test`, `cargo test --test shaders`, and `cargo build --release --locked` all exit 0.
- [ ] A dense-city car run holds 60 fps at 1080p on the mid-range target, and the flight benchmark does not regress.

## Assumptions and open questions

- **Landcover endpoint.** The ESA WorldCover z14 PNG endpoint was verified by hand for Buenos Aires and Paris, returning raw `LA` class values. Production rate limits and long-term endpoint stability are not guaranteed; the cache and OSM fallback are mandatory.
- **WorldCover licensing.** ESA WorldCover 2021 v200 is treated as CC BY 4.0. The exact attribution wording should be checked against the ESA WorldCover terms before release.
- **Parked aircraft.** Traffic types 0 (light single) and 1 (business jet) map to `c172` and `g650`. Larger parked types are scenery and use the airport menu fallback. This mapping is an implementation choice, not a user requirement.
- **Tunnels.** V1 keeps tunnels at grade with no interior. They are excluded from gameplay; this is a known limitation.
- **Traffic drivers.** Traffic cars have no visible drivers. Carjacking a moving car makes it yield and stop before the player enters. This follows from the no-pedestrian scope.
- **Save scope.** The save is a resume snapshot, not a simulation save. Traffic positions, debris, and full weather particles are not restored.
- **New CLI.** `--start`, `--flat`, `--surface-probe`, and `--no-save` are new. Existing flight flags keep their behavior.
- **Crate rename.** The rebrand renames the Cargo package and binaries to `gtw`, `gtw-mcp`, and `gtw-launcher`. The relay's `mp_proto` include path is unaffected.
- **No new crates.** If a COG dependency becomes necessary, it is an explicit change to this plan, not an implementation detail.

## References

- FableFlight upstream: <https://github.com/Rolandjg/fableflight>
- GTW fork: <https://github.com/galatolofederico/gtw>
- ESA WorldCover data access: <https://esa-worldcover.org/en/data-access>
- ESA WorldCover on Planetary Computer: <https://planetarycomputer.microsoft.com/dataset/esa-worldcover>
- Planetary Computer data API: <https://planetarycomputer.microsoft.com/docs/quickstarts/using-the-data-api/>
- Existing world and surface code: `src/world.rs`, `src/sources.rs`, `src/roads.rs`, `src/buildings.rs`
- Existing traffic and models: `src/cars.rs`, `src/traffic.rs`, `src/airmodels.rs`
- Existing game loop, camera, input, HUD: `src/main.rs`, `src/bind.rs`, `src/settings_ui.rs`
- Existing rendering: `src/render.rs`, `src/shaders/inst.wgsl`, `src/shaders/terrain.wgsl`
- Existing audio: `src/audio.rs`
- Repository rules and verification: `AGENTS.md`, `README.md`, `Cargo.toml`, `tests/shaders.rs`, `tests/mcp.rs`
