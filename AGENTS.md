# DeepTek agent notes

DeepTek is a Rust 2024 Terraria-style terrain prototype. Keep systems modular and
performance-conscious; do not add unrelated gameplay or visual features unless asked.

## Architecture

- `src/terrain`: fixed worlds up to 64 million cells (`10,000` max width,
  `16,000` max depth), dense `64 × 64` chunks,
  foreground/background `u16` tile layers, deterministic parallel generation, and
  checked RLE save/load. `objects.rs` owns persistent anchored decorations, spatial
  indices, and the budgeted growth-event queue.
- `src/terrain_renderer`: `easy-gpu` chunk meshes streamed around the player. Mesh
  generation is multithreaded; GPU creation/removal stays on the render thread.
  `decorations.rs` renders resident grass, pebbles, and vine segments as one lit
  instanced batch using `assets/decorations/deco.png`.
- Marching-square UVs use the exact legacy 256-entry LUT in `lut.rs` and the original
  tile atlases under `assets/terrain`.
- `src/terrain_renderer/lighting.rs`: the legacy compute-lighting model—occupancy,
  sky/dynamic sources, 12 diffusion passes, smoothing, 2× upscale, and occlusion.
- `src/main.rs`: minimal movable-camera demonstration only.

Terrain edits should go through `TerrainRenderer::set_tile`, or be followed by
`mark_tile_dirty`, so neighbouring meshes and lighting are refreshed correctly.
Foreground changes made through `World::set_tile` also remove any object rooted in
the broken tile. `World::update_nature` advances growth and performs bounded active-area
grass/vine spawning and dirt-to-grass spreading; dirty its returned foreground tile
positions in the renderer.

## Verification

Before finishing changes, run:

```powershell
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

Shader or GPU-binding changes also require a brief `cargo run --release` smoke test,
because WGSL and pipeline validation occurs at runtime.
