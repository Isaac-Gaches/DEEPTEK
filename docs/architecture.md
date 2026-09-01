# Architecture and source layout

DeepTek separates persistent simulation state from presentation. The library in
`src/lib.rs` owns reusable systems; the executable in `src/game/app` wires those systems
to input, menus, rendering, and save jobs. Simulation code must not depend on renderer
residency, so off-screen machinery and world state remain authoritative.

## Directory map

```text
src/
  engine/                Renderer-independent authoritative simulation
    entity/              ECS, player, physics, lifeforms, hazards, and turrets
    item_transport/      Conveyor and machine-port transfer simulation
    items/               Item registry, inventories, drops, and item use
    power/               Power topology, localized edits, and distribution
    terrain/             World, generation, objects, nature, surveys, and persistence
  render/                GPU and GUI presentation
    entity_renderer/     Sprite instance collection and rendering
    gui/                 HUD and retained/batched game windows
    post_process/        Screen-space post-processing
    sky_renderer/        Sky rendering
    terrain_renderer/    Streamed terrain, furniture, decoration, and lighting
  game/                  DeepTek-specific rules, content, and application shell
    app/                 Input, menus, frame orchestration, rendering, and save jobs
    content/             Discoverable content facade and crafting definitions
    machine_processing/  Processor recipes and processing ticks
    specialists/         Housing, recruitment, happiness, and persistence models
    contracts.rs         Contract definitions and runtime board
    delivery.rs          Procurement offers and cargo drops
    orbital_export.rs    Export rules
    transmissions.rs     Transmission state
    tutorial.rs          Tutorial definitions and progression rules
```

`src/lib.rs` uses explicit module paths so existing public crate paths remain stable
despite this physical ownership split. `game/content` re-exports all built-in definition
tables as a single authoring entry point.

Assets are grouped by rendered subject beneath `assets/`. Developer-facing guides live
in `docs/`, the headless stress harness lives in `benches/`, and the vendored renderer
dependency is isolated in `vendor/easy-gpu`. Build output, IDE state, and local world
saves are excluded from version control.

## Dependency direction

1. `engine` owns authoritative state and renderer-independent mechanics.
2. `game` defines DeepTek-specific content and coordinates simulation systems.
3. `render` reads state and produces presentation data.
4. `game/app/session` schedules systems and translates input into domain operations.

Code in `engine` must not import the app, GUI, or renderer modules. Terrain
edits made while a renderer is active go through `TerrainRenderer::set_tile`, or the
affected tile is explicitly dirtied afterward, so adjacent meshes and lighting update.

## Module conventions

- A module with child modules uses `name/mod.rs`; avoid maintaining both `name.rs` and a
  sibling `name/` directory.
- Keep tests beside the system they exercise, normally in `tests.rs` under that module.
- Split by responsibility rather than file length alone. A split should give the new
  module a clear owner and a narrow interface.
- Re-export intended public types from `src/lib.rs`; keep implementation helpers private
  to their owning module.
- Preserve stable item, object, tile, lifeform, and save-format identifiers.
- Prefer indexed, active-area, or explicitly budgeted work over scans proportional to
  total world size.

## Where new work belongs

- Persistent cells, objects, generation, durability, surveys, or save data: `engine/terrain`.
- ECS behavior or components: `engine/entity`.
- Inventory operations, drops, or held-item mechanics: `engine/items`.
- Game definitions, progression, recipes, or economy rules: `game`.
- A window or HUD element: `render/gui`; app navigation remains in `game/app`.
- GPU resources or draw preparation: the appropriate `render` module.
- Cross-system frame ordering and user-event routing only: `game/app/session`.

See `adding-content.md` for registry-specific extension steps and
`stress-benchmarks.md` for performance validation.
