# DeepTek

DeepTek is a Rust 2024, Terraria-style mining and automation prototype. Players descend
through a deterministic chunked world, establish powered extraction infrastructure,
complete the DEEPTEK Prospector Program, and work toward recovering Asterite around
10 km below the surface.

The project currently includes terrain generation and persistence, mining and building,
power and logistics networks, automated machinery, orbital exports, deliveries,
contracts and transmissions, specialists and housing, hostile lifeforms, and a streamed
GPU renderer with dynamic lighting.

## Running

The repository vendors `easy-gpu`, so no additional engine checkout is required.

```powershell
cargo run --release
```

Development checks:

```powershell
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

World saves are written beneath `worlds/` and are intentionally excluded from version
control.

## Controls

| Input | Action |
| --- | --- |
| A/D or Left/Right | Move |
| Space | Jump |
| W/S | Climb ropes and powered cables |
| X | Interact, or open inventory when no interaction is available |
| I | Toggle inventory |
| 1–0 or mouse wheel | Select hotbar slot |
| Left click | Use the selected item or operate inventory slots |
| Right click | Alternate placement/inventory action |
| `=` / numpad `+` | Smoothly zoom in |
| `-` / numpad `-` | Smoothly zoom out |
| Tab | Toggle world map |
| Escape | Close the current window or pause |

The pause menu contains Medium and High render-distance settings tuned for bounded lighting
textures on integrated GPUs. The maximum camera zoom-out automatically scales to the selected
streaming distance and viewport.

## Gameplay overview

New prospectors begin in a generated landing pod with a mining tool, rope, glowsticks,
and a bed. The tutorial is presented as persistent contracts in the Contracts window.
Requirements count retroactively where practical, and completing early missions unlocks
equipment deliveries and the wider Prospector Program.

Delivery timers appear in the lower-left corner. When a timer expires, the crate enters
near the local surface with a short randomized fall, horizontal drift, and tumble.
Incoming transmissions remain visible until dismissed and are retained in the
Transmissions tab.

Solar arrays and orbital exporters require clear sky access and only operate above
-100 m. Terminals require a small powered connection. Conveyors transfer resources
between compatible machine inputs, outputs, and buffers without relying on renderer
residency, so remote machinery continues to operate.

Specialists require suitable enclosed accommodation with background walls, a door, and
a bed. An open door still counts as a housing boundary. Beds in valid housing can also
be used by the player to pass the night.

## Architecture

- `src/engine/terrain` owns fixed chunked worlds, deterministic generation, foreground and
  background tiles, persistent objects, structures, biomes, durability, surveys, nature,
  and checked save/load.
- `src/render/terrain_renderer` streams terrain meshes around the player and renders furniture,
  decorations, the 8-bit light map, and occlusion. Mesh preparation is multithreaded;
  GPU resource changes remain on the render thread.
- `src/engine/entity` contains ECS components and systems for the player, physics, effects,
  lifeforms, hazards, projectiles, and turrets.
- `src/engine/items` contains item mechanics, inventory, dropped items, and item-use
  dispatch.
- `src/render/gui` contains retained batched interfaces for the HUD, inventory, contracts,
  transmissions, procurement, specialists, and world map.
- `src/game/app` is the thin playable-application shell: menus, input, interaction, save
  jobs, frame orchestration, and runtime rendering.
- `src/game/content` is the authoring entry point for built-in definitions. Other game
  modules contain processing, delivery, contracts, specialists, and tutorial rules.

Worlds use dense `32 × 32` chunks with separate `u16` foreground and background planes.
The supported ceiling is 10,000 tiles wide, 16,000 tiles deep, and 64 million cells.
Generation is deterministic across worker counts. Saves use versioned, checksummed RLE
sections and validate dimensions, runs, object relationships, and trailing data before
loading.

Terrain changes should go through `TerrainRenderer::set_tile`, or be followed by
`mark_tile_dirty`, so neighbouring marching-square meshes and lighting are refreshed.
Simulation systems operate on world state rather than visible chunks.

## Extending the game

See [Adding content](docs/adding-content.md) for the registries and validation steps used
to add blocks, furniture, machines, decorations, and lifeforms.

See [Architecture](docs/architecture.md) for the complete source layout, dependency
boundaries, module conventions, and guidance on where new code belongs.

See [Stress benchmarks](docs/stress-benchmarks.md) for benchmark commands, presets, and
how to interpret the simulation timing report.

## Repository policy

- Keep stable numeric IDs stable once they have appeared in a save.
- Keep systems modular and independent of renderer residency.
- Bound work by active chunks, indexed objects, or explicit per-frame budgets rather
  than total world size.
- Update persistence migration and corruption tests whenever the world format changes.
- Do not commit IDE state, build output, or local world saves.
