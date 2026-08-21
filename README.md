# DeepTek terrain core

A rendering-independent, fixed-size 2D tile world written in Rust. Its deterministic
integer-density generator produces rolling hills, mountain chains, cliffs, surface
overhangs, small cave pockets, and long winding tunnels. Biome, ore, structure, and
liquid passes can be layered on later without replacing storage or persistence.

## Architecture

- Worlds retain the `10,000`-tile legacy width ceiling, allow depths up to `16,000`,
  and cap every allocation at 64 million tiles. Menu presets are at most `4,000 × 16,000`.
  Storage is divided into `64 × 64` chunks.
- Each chunk contains dense foreground and background `u16` tile planes. A maximum
  world uses about 244 MiB for raw tile data before chunk-edge padding and object state.
- A tile ID is data rather than an enum, so game-specific registries can grow without
  changing the world format. ID `0` is empty on both layers.
- Chunk order and tile order are deterministic. Generation gives identical output
  for a seed regardless of worker count.
- Generation precomputes only a compact per-column profile; chunks sample bounded
  integer noise independently, with no world-sized temporary density allocation.
  Generation, save compression, and load decompression partition chunks over scoped
  standard-library worker threads.
- World files have a signature and format version, fixed metadata, RLE-compressed
  planes, and checksummed terrain/object sections. Loading validates all sizes,
  runs, object footprints, checksums, and trailing bytes before returning a world.
- Saving writes and flushes a sibling temporary file before replacing an existing
  save, with a backup/restore step for Windows-compatible replacement.

Chunk-local planes are exposed for future high-throughput systems. Ordinary gameplay
code can use bounds-checked `tile` and `set_tile` calls.

## Basic use

```rust
use deep_tek::{ForegroundTile, Layer, World, WorldGenerator};

let mut world = WorldGenerator::new(12345).generate(256, 128)?;
world.set_tile(100, 50, Layer::Foreground, ForegroundTile::AIR)?;
world.save("worlds/first.world")?;

let loaded = World::load("worlds/first.world")?;
# Ok::<(), deep_tek::WorldError>(())
```

`generate`, `save`, and `load` automatically use available CPU parallelism. Their
`with_threads` variants make worker counts explicit for tests, profiling, or a future
shared game job scheduler.

## Terrain renderer

The `terrain_renderer` module is a deliberately small `easy-gpu` renderer for the two
terrain layers. It keeps GPU meshes only for chunks near a supplied player position,
uses a configurable hysteresis margin before despawning them, and caps mesh rebuilds
per frame. CPU mesh generation runs on a retained Rayon worker pool; creating and
removing `easy-gpu` mesh handles stays on the render thread. Resident chunk geometry
is combined into local 2×2 regions, reducing terrain submissions by up to four times
without forcing distant edits to rebuild one world-sized mesh.

Each occupied tile emits one quad. Its eight-neighbour mask is mapped through the
exact 256-entry marching-squares LUT from the earlier project. Foreground and
background materials use the original nearest-filtered tile atlases, embedded from
`assets/terrain` at compile time.

Use `TerrainRenderer::set_tile` when possible. It updates the world and marks the
owning render chunk dirty, together with only the edge or diagonal chunks whose
marching-squares masks are affected. If another system edits `World` directly, call
`TerrainRenderer::mark_tile_dirty` afterward.

Running `cargo run --release` opens the minimal terrain demonstration. A/D or the
left/right arrows move the player; Space, W, or the up arrow jumps. The camera follows
the player while terrain chunks and lighting continue to stream around the player's
actual position. Press `=` (or numpad `+`) to zoom in and `-` (or numpad `-`) to zoom
out. Zoom uses bounded multiplicative steps so camera scale remains valid.

The demo opens on a lightweight world menu. `Create New World` opens a creation form for
the display name, numeric seed, and one checked size preset: Small is `2,000 × 8,000`,
Medium is `3,000 × 12,000`, and Large is `4,000 × 16,000`. The selected dimensions are
passed directly to deterministic generation. Save filenames remain collision-free numbered files under
`worlds/`, while their embedded display names are shown in the list. Each row has a delete
button followed by a separate permanent-deletion confirmation screen. The mouse wheel
scrolls longer save lists. Active worlds autosave every 60 seconds on a two-thread
background worker and save synchronously once more during a clean exit. A short on-screen
notice reports completed or failed saves.
World files currently persist terrain, natural decorations, and nature simulation time;
player entities and inventory remain demo-session state.

Press Escape in a world to open the pause menu. Gameplay and world simulation stop while it is
open. `Resume` returns to play, while `Save and Main Menu` performs a checked synchronous save and
only closes the world after that save succeeds.

The compact top HUD shows the player's current/maximum health and energy, available money, and
buttons for Contracts and Pause. The contracts button opens a modal board and pauses simulation;
each entry contains a requirement, monetary reward, and issuing company. Item-export contracts
also show live exported/required progress. Escape or Close returns to play. Health, energy,
money, contract progress, player entities, and inventory are currently
demo-session state rather than world-save data.

The bottom hotbar is selected with 1-0, the mouse wheel, or a click. Press E to expand
the full inventory. While it is open, left click moves or merges stacks and right click
splits a stack or places one item. Left clicking in the world uses the selected item:
blocks are placed, the pickaxe removes foreground tiles, and consumables dispatch their
configured effect. The starter inventory also includes wooden chests, the laser bore, the
defence turret, the orbital export launcher, cargo conveyors, solar arrays, pylons, and batteries.
Select the chest and
click the lower-left air tile of a valid 2x2 placement above two solid floor tiles. The
preview covers the full footprint, and the pickaxe can remove the chest from any of its
four occupied cells. Right click any chest cell to open its 40-slot storage beside the
player inventory. Left click moves or merges stacks and right click splits or transfers
one item; E closes the chest. Cargo conveyors may start from any side of a machine with
an item input, output, or storage buffer, then extend horizontally or vertically without
terrain support. Straight and corner sprites connect automatically; junctions are not
currently supported. Glow sticks are thrown toward the cursor, collide and bounce against
foreground terrain, and contribute green dynamic light until their five-minute lifetime
expires. They spin rapidly in the direction of the throw. The prototype does not impose
an interaction radius. Bombs use the same throwing physics, burn for three seconds, and
remove foreground terrain within an eight-tile circular blast. Each explosion emits
batched, bouncing orange sparks with fading dynamic light and rising smoke particles.
Callers can
pass a finite reach to the item-use system when a tool or game mode needs one.

## Items and inventory

`ItemRegistry` stores stable numeric IDs and immutable `ItemDefinition` values. Each
definition supplies its category, icon, maximum stack size, per-item export value, and an `ItemAction` such as
placing a tile, invoking a tool action, consuming an effect, or dispatching a custom
action ID to a future gameplay system. Inventory and GUI code do not need per-item
branches when new definitions are registered. Export value defaults to zero and can be
set fluently with `ItemDefinition::with_export_value`.

`Inventory` contains 40 slots, with its first ten forming the hotbar. Addition fills
compatible partial stacks before empty slots and returns any overflow. Quantities use
`u16`; registry stack limits are enforced during additions, merges, splits, and
single-item placement. The GUI batches slot backgrounds, item icons, and bitmap digits
by material and issues a small fixed number of instanced draws.

## Entities, physics, and sprites

Gameplay entities live in a `hecs::World`. `Transform`, `Sprite`, and `Collider` are
small, composable components; the `Player` component and its input system only modify
collider velocity. Tile physics resolves each axis independently and examines only
the collider's local foreground-tile footprint. Displacement-sized substeps prevent
fast bodies from tunnelling without tying collision work to total world size.

`SpriteRenderer` groups every `Sprite + Transform` query result by material, retains
the instance vectors between frames, and issues one instanced draw per material. Its
materials support atlas frames and reuse the terrain camera and lightmap, so sprites
are lit and occluded consistently with foreground terrain. GPU handles and materials
are still created exclusively on the render thread. Entity, GUI, and decoration
renderers share one unit-quad definition rather than maintaining duplicate geometry.
Bomb, spark, and smoke behavior is isolated in the ECS effects system. Sparks reuse tile
physics while smoke uses a lightweight free-particle update; both reuse one particle
material and therefore remain a single instanced sprite batch.

`LifeformSystem` is a data-driven ground-lifeform controller. New species register a
`LifeformDefinition` containing health, collider, appearance, movement, and stuck-jump
tuning. Runtime entities remain ordinary `Lifeform + Transform + Collider + Sprite`
compositions. Walkers accelerate toward the player, use reduced air control, detect both
wall contacts and lack of horizontal progress, and jump after a configurable stuck delay
and cooldown. The demo spawns four tinted walkers around the player when a world starts.

## Natural decorations

Generation deterministically seeds sparse grass, grass/stone pebbles, and vines beneath
existing grass overhangs in a persistent object store. Active-area nature ticks
gradually add more grass, create vines at newly valid overhangs, and spread exposed
grass tiles into adjacent dirt.
Objects have stable IDs, an anchor, a separate supporting root, an occupied rectangular
footprint, variant/growth state, and a scheduled next update. The general footprint
and type-ID representation is intended to extend to multi-tile furniture and
interactables without changing terrain storage.

## Furniture

Player-placeable furniture shares the persistent object store and its O(1) occupancy
lookups. `FurnitureDefinition` supplies a stable object type, rectangular size, support
rule, and interaction kind. The built-in chest is a floor-supported 2x2 container. Its
entire footprint must be empty, every floor cell must be solid, tile placement cannot
overlap it, and breaking either support removes an empty chest. Non-empty chests cannot be
mined, overwritten, exploded, or unsupported, preventing stored items from being silently
lost. `World::object_at` and
`World::furniture_interaction_at` resolve the whole object from any footprint tile, so a
future storage or furniture UI does not need to scan nearby objects.

Furniture and container slots are saved in checked object records and drawn in a
resident-chunk instanced batch using the world footprint for scale. Furniture sprites are
lowered by 0.08 tile so their base sits slightly into supporting terrain. To add another floor-supported size,
define a stable `FurnitureObject` ID and `FurnitureDefinition`, register an item with
`ItemAction::PlaceFurniture`, and add its final visual as another equal-width furniture
atlas frame. `FurnitureInteraction::container`, `machine`, and `controlled_machine`
declare reusable storage and controls without coupling container behavior, simulation,
or UI layout to a specific furniture type.

All furniture containers use the same persistent object-ID-keyed storage section. Every
slot, item ID, and quantity is saved regardless of container type or distance from the
player. Saving validates that each container furniture object has exactly one correctly
sized record and rejects missing, orphaned, or non-container storage rather than writing a
partially persistent world.

The orbital export launcher is a floor-supported 3x3 container with eight slots. Every four
seconds, each launcher removes the first complete stack in slot order and emits a shipment.
The player's wallet receives the item's definition-owned export value multiplied by its stack
quantity; unknown or deliberately unpriced items still launch but pay zero. Shipments are
allocated across matching unfinished contracts in display order, so no exported item is counted
twice. Completing a contract deposits its reward exactly once. `OrbitalExportSystem` finds
launchers through the object type index, so its update cost depends on launcher count rather than
planet size or decoration count, and consumers can handle its typed `ExportShipment` events
without coupling economy logic to furniture storage.

Cargo conveyors are floor-supported 1x1 transport connectors. Place a continuous line so its
end cells touch any footprint cell of the source and destination furniture. Once per second,
each connected network transfers at most one complete stack using definition-owned roles:
laser-bore output goes to an orbital-launcher input first, otherwise to chest buffer storage;
buffer storage can feed an input when one is available. Inputs never send items backward into
outputs, and a full destination leaves the source stack untouched.

`ItemTransportSystem` derives connected components from the sparse furniture occupancy index.
It rebuilds only after connector or container furniture is placed or removed—not when grass,
vines, configuration, or inventory contents change. Steady-state frames do constant work until
the one-second cadence is due, then process only known networks and endpoints. No conveyor item
entities are spawned and renderer residency is never consulted, so drills and cargo networks
continue working anywhere in the planet. Long-frame catch-up is capped at two transfer ticks.

The built-in solar array is a floor-supported 2x3 generator, the pylon is a floor-supported
1x2 relay, and the battery is a floor-supported 2x2 store. Each pylon automatically considers
definition-declared generators, stores, and consumers, plus other pylons, whose sockets are within
15 tiles. Foreground terrain blocks the supercover line-of-sight traversal, including diagonal
corner crossings. Solar arrays generate 12 power units per second only during the daylight portion
of the sky cycle. Laser bores, loaded orbital exporters, and active defence turrets consume 8, 4,
and 6 units per second respectively. Surplus generation fills each battery up to 240 units;
batteries discharge deterministically to cover nighttime or overloaded demand, and charge survives
save/load. If supply is insufficient, consumers are selected in stable object-ID order. Energized
pylons remain subtle flickering cyan light sources.

`PowerSystem` discovers nodes through the sparse furniture type indices, uses 15-tile spatial
buckets instead of comparing every node with every other node, and rebuilds candidates only when
power furniture is placed or removed. Candidate sight lines are indexed by their crossed terrain
tiles. A foreground edit therefore rechecks only affected short edges, and the union-find network
is recomputed only if an edge actually opens or closes. A bounded terrain-change journal safely
falls back to rechecking all candidates if a system misses too many edits. The renderer reads only
connections indexed into resident chunks and draws all visible cables as one lit instanced batch;
ten short segments follow a shallow parabola to provide a slight sag.

The built-in laser bore is a 3x3 floor machine supported by its two outside feet, leaving
the centreline clear. A newly placed bore is off: right-click it and press `Activate` in its
inventory window to start it, or `Deactivate` to stop it. This enabled state survives save/load;
the machine still requires an energized pylon connection. While active and powered, its emissive
`#00ffff` beam scans straight down through at most 400
foreground cells, stops at the first solid tile, and destroys that tile after three
one-second scheduled updates. Each traversed beam cell seeds the existing diffused
lightmap with cyan light. Beam geometry and its bounded light-source list are rebuilt
together only when resident objects or foreground collision changes; regular lighting
refreshes reuse the cached list without scanning the world. The cyan illumination uses a subtle,
smooth deterministic flicker. Powered pylons add a much dimmer cyan source with a slow occasional
dip, both through the same cached lighting input. Mining progress and the next
scheduled update use the same persistent object record as natural growth. Mined dirt,
grass, stone, and built-in light blocks are deposited atomically into a ten-slot internal
container. Right-clicking the bore opens its one-row inventory; if it is full or a custom
tile has no registered drop, mining pauses without destroying that tile. Destroyed tile
positions are returned through `NatureUpdate::changed_tiles()` so terrain meshes, beam
geometry, and lighting are dirtied locally. Bore events live in the global scheduled-object
heap, so mining continues at full cadence when the player and resident renderer chunks are
far away; no terrain or object scan is performed to find off-screen bores.

Resident laser bores emit a bounded impact-only particle burst eight times per second: dense bright
cyan fragments and a larger warm dust cloud kick away from the struck block, with no particles
leaking from the bore itself. Emission is capped to 16 resident bores and two catch-up pulses per frame.
Off-screen bores still mine and collect items but do not create invisible particle entities.

The built-in defence turret is a 2x2 controlled machine and also starts off. Right-click
it to open its control panel, activate or deactivate it, and choose Weakest, Strongest,
Closest, or Furthest targeting among living `Lifeform` entities inside its range. The
active flag and typed target priority both survive save/load; the turret deliberately has
no fake storage container. `TurretStats` centralizes its designer-adjustable range, firing
interval, projectile speed, gravity, lifetime, and damage, and `TurretSystem::set_stats`
can replace those values without touching UI, persistence, or targeting code. The low-arc
launch solver finds a fixed-speed ballistic solution when one exists. Each round then uses
the exact constant-acceleration SUVAT updates `s = ut + 1/2 at^2` and `v = u + at`, with
swept terrain/lifeform collision to prevent tunnelling. Weakest and Strongest compare
current health; deterministic distance and entity-ID tie breakers keep results stable.
Before selecting, the turret performs a tile-grid sight traversal and discards candidates
hidden by foreground terrain, so the next visible candidate under the selected priority is
used instead. Lifeforms are bucketed into local 16-tile cells, turret candidates are read
only from world chunks near those lifeforms, and projectile collision queries the same
local buckets. Turrets and lifeforms on opposite sides of a world therefore do not enter
one another's targeting work. Each fired round also emits a fixed four-particle cyan muzzle
burst through the existing instanced particle renderer. The round and muzzle flecks are self-lit,
feed colour-selective bloom, and contribute short-lived cyan dynamic light.

Terrain, furniture, decoration, and entity shaders treat exact red, green, blue, yellow, cyan, and
magenta texels as self-lit colour keys. Those six endpoint colours bypass terrain illumination and
are the only colours selected by the bloom pass; black and white remain ordinary non-glowing art.
Successful orbital exports also emit a fixed upward burst of self-lit cyan sparks from the launcher.

## Adding content

The main extension points are deliberately definition-driven:

| Content | Add the definition in | Runtime path reused automatically |
| --- | --- | --- |
| Block | `src/terrain/blocks.rs` and `src/items/registry.rs` | placement, mining drops, persistence, terrain meshing |
| Furniture or machine | `src/terrain/furniture.rs` and `src/items/registry.rs` | footprint checks, occupancy, interaction controls, persistence, item placement |
| Natural decoration | `src/terrain/decorations.rs` | object storage, spatial indexing, persistence, resident decoration batching |
| Enemy/lifeform | a `LifeformDefinition` registered with `LifeformSystem` | spawning, health/collider composition, movement, turret targeting, sprite batching |

Reserve stable numeric IDs, keep balance and appearance in the corresponding definition,
and put behavior that genuinely differs in a focused system module. This keeps generic
inventory, world storage, render batching, and save validation free of per-content branches.

Root and occupancy hash maps make tile edits local: breaking a supporting foreground
tile immediately removes every object rooted there, without scanning the world.
Per-chunk indices feed only resident objects to the renderer. Growth uses a min-heap
of due events and a caller-supplied work budget, so static decorations cost nothing
per simulation tick and large catch-up updates can be spread across frames. The object
store carries a lightweight visual revision, so resident decoration instances rebuild
only after object changes or visible-chunk transitions rather than every frame.

Call `World::update_nature(elapsed, active_position, config)` with elapsed game time.
`NatureSimulationConfig` exposes the active radius, columns examined per one-second
tick, catch-up cap, global object-event budget, and spawn/spread chance divisors. The
active radius applies only to natural column scans; scheduled object events are independent
of player position. Defaults
scan only eight active columns per second (at most 32 after a long frame), keeping the
work independent of total world size. Pass returned `changed_tiles()` to
`TerrainRenderer::mark_tile_dirty` so grass spreading refreshes only affected meshes
and lighting. Decoration state, vine length, stable IDs, and the simulation clock
survive save/load. The renderer expands a vine into compact 16-byte segment instances,
samples the original five-frame `deco.png` atlas, and uses the same terrain lightmap
and opaque pipeline behaviour as the legacy project.

## Lighting

The terrain renderer includes the original compute-based lighting model. A
camera-aligned tile window encodes air, foreground, and background occupancy. Sky and
dynamic light sources seed a tile-resolution lightmap, foreground tiles block its 12
diffusion passes, two tile-resolution smoothing passes run, and the result is upscaled
2× before two more smoothing passes. Foreground tiles sample the resulting RGB light;
background tiles retain the original reduced-light and occlusion treatment.

Tile IDs `4` and `6` retain the original red and blue built-in light colours.
Additional lights can be passed as `LightSource` values to
`TerrainRenderer::update_lighting`. `set_sky_light` controls the sky contribution,
and tile edits made through the renderer automatically request a lighting refresh.
The example recomputes at 20 Hz (every 50 ms).

## Sky and day/night cycle

`SkyRenderer` ports the legacy fullscreen sky into a separate rendering module. It
blends the original day, low-sun, and night palettes across a configurable clock,
draws deterministic twinkling stars at night, and layers moving procedural clouds over
the gradient. Its ambient colour is passed to `TerrainRenderer::set_sky_light` on the
existing lighting cadence, so terrain, decorations, and entities darken and warm with
the visible sky without adding an extra lighting compute pass per rendered frame.

`SkyRenderConfig` defaults to the legacy 500-second day, a sunset starting time, and
1,000 stars. `set_time_of_day` can jump the normalized clock to any desired point.
The demo caps rendering at 60 FPS, pauses while unfocused, and uses a camera-sized
five-by-three chunk lighting window. Clouds share the gradient pass and skip procedural
work below their visible region, keeping the effect practical on integrated laptop GPUs.

## Adding content

Stable definition tables now provide shared metadata for blocks, furniture, natural decorations,
items, and enemies. See [`docs/adding-content.md`](docs/adding-content.md) for the minimal steps and
the cases that genuinely require new system behaviour.

## Format evolution

The file format is currently version 8. Version 1 terrain-only saves remain loadable and
start with an empty object store. Version 2 object saves also remain loadable; existing
container furniture is migrated with empty storage. Version 3 container saves remain
loadable. Version 4 adds a small bounded world-name extension directly after the fixed
header, allowing the menu to read display names without decompressing terrain. Version 5
adds session time and player position. Version 6 adds laser-bore storage; older saved bores
receive an empty ten-slot container during load. Version 7 persists generic object activation;
older laser bores migrate in the safe off state. Version 8 adds persistent fixed-point battery
charge and a full-width laser target coordinate for the 400-tile bore range. When more
metadata, entities, or generator settings are added, increment the version and add a
migration path rather than silently changing existing fields. Foreground/background tile
IDs, item IDs, and object type IDs are stable integer values.
