# Adding game content

DeepTek keeps stable IDs and immutable definitions separate from runtime state. The
`game_content` module in `src/game/content/mod.rs` is the index of authoring tables. Reserve an ID,
add one definition to the appropriate table, add atlas art when needed, and only add system code
when the content introduces genuinely new behaviour. Never reuse an ID from a saved world.

## Blocks

1. Reserve a foreground `TileId` constant beside `ForegroundTile` in `src/engine/terrain/mod.rs`.
2. Add a `BlockDefinition` to `BUILT_IN_BLOCKS` in `src/engine/terrain/blocks.rs`. This is the shared
   source for mined drops and emitted light.
3. If players can carry or place it, reserve an `ItemId` and add an `ItemDefinition::block` in
   `built_in_item_definitions` (`src/engine/items/registry.rs`). Chain `with_export_value` when it should
   sell for more than the default value of zero.
4. Add the matching tile frame to `assets/terrain/fg_tiles.png`. Foreground tile ID N uses atlas
   frame N; preserve existing frame positions. Background-only tiles use the background atlas and
   do not need foreground gameplay metadata.

Unknown numeric tiles still round-trip through persistence, so staged content does not corrupt a
world. Register a drop before expecting the laser bore to mine that block.

## Furniture and machines

1. Reserve an `ObjectTypeId` in `FurnitureObject` and add one `FurnitureDefinition` to
   `BUILT_IN_FURNITURE` in `src/engine/terrain/furniture/mod.rs`.
2. Choose `FurnitureInteraction::container(slots)` for storage or
   `FurnitureInteraction::machine(slots)` for storage plus the generic Activate/Deactivate UI.
   Machine enabled state is saved automatically and newly placed machines start off. For cargo
   routing, use `container_with_transport` or `machine_with_transport` and declare the endpoint
   as `ItemTransportRole::Input`, `Output`, or `Buffer`.
   To participate in power, chain `with_power(PowerRole, socket_half_tiles)`, followed by
   `with_power_rate` for generators/consumers or `with_power_capacity` for storage. Relays are the
   automatic 22.5-tile connection points; `PowerSystem::distribute` budgets generated and stored
   fixed-point energy, after which consumers query `PowerSystem::is_powered`. Socket offsets use
   exact half-tile units relative to the top-left furniture anchor.
3. For a carryable item, chain `with_item(FurnitureItem { ... })` on that furniture definition.
   Its inventory definition, placement action, stack limit, icon, and export value are generated
   automatically. To sell it, also chain `with_offer(FurnitureOffer { ... })`; it then appears in
   procurement with the declared company, price, unlock level, and description. Do not add a
   parallel entry to `built_in_item_definitions` or `MACHINE_OFFERS`.
4. Add its world frame to the active furniture atlas and update the atlas dimensions in
   `src/render/terrain_renderer/furniture/mod.rs` when adding a row or column. Furniture with a
   procedural visual should add that visual to the same renderer module instead of reserving an
   unused atlas frame.
5. Select a reusable `FurnitureBehavior` for bores, cargo lifts, and turrets. New variants of
   those families are discovered by their definition and reuse their simulation. Put genuinely
   new behavior in a focused system. Use `World::objects_of_type` for sparse global machines, or
   the scheduled-object path when behavior naturally follows simulation ticks.
   Occupancy, support validation, containers, save/load, placement preview, removal, and the
   interaction window are generic.

An item-transport connector needs no container or update branch: call
`FurnitureDefinition::with_item_transport_connector`. The cargo system discovers all connector
definitions, builds components from O(1) occupancy lookups, and connects them to any adjacent
definition-declared endpoint. Furniture topology has a dedicated revision, so natural object
growth does not trigger network rebuilds.

## Natural decorations

1. Reserve an `ObjectTypeId` in `NaturalObject` and add a `DecorationDefinition` to
   `BUILT_IN_DECORATIONS` in `src/engine/terrain/decorations.rs`.
2. Use `DecorationVisual::Static`, `Variants`, or `Segmented`; these render without a new
   renderer branch. Add the referenced frames to `assets/decorations/deco.png`.
3. Add generation or active-area spawning rules only if the decoration should appear naturally.
   A decoration with custom growth behaviour also needs a scheduled-update branch.

## Crafting recipes

Add ingredient slices and one `CraftingRecipe` to `CRAFTING_RECIPES` in
`src/game/content/crafting.rs`. The crafting menu reads this table directly, so a recipe needs no
GUI edit. Every referenced item must be registered.

## Contracts and tutorial missions

- Add ordinary rotating contracts to the single vector in `built_in_contracts` in
  `src/game/contracts.rs`; the Contracts menu consumes that vector automatically.
- Add tutorial display data to `TUTORIAL_MISSIONS` in `src/game/tutorial.rs`. Assign a new,
  never-reused `progress_bit` so old saves keep their meaning, then add the mission's unlock and
  progress rule in the focused methods immediately above the table. Definition validation tests
  catch duplicate bits or missions missing from the program.

## Specialists

Reserve a stable `SpecialistId` and add one `SpecialistDefinition` to
`BUILT_IN_SPECIALISTS` in `src/game/specialists/mod.rs`. Recruitment, persistence, the specialist
menu, and housing bonuses all discover the definition from that table.

## Enemies

1. Reserve a stable `LifeformId` beside the built-in table in `src/game/content/lifeforms.rs`.
2. Add one `LifeformDefinition` to `BUILT_IN_LIFEFORMS` in the same file. Start with the const
   `walker` or `glowgnat` template and override fields in a named const when more tuning is needed.
   The same definition owns movement, health, attacks, spawning biomes and weight, machinery
   attention, stuck/random jump timing, tint, emissive light, scale, and visual style. Set
   `random_jump_interval` to `[0.0, 0.0]` for species that should never jump ambiently.
3. `LifeformSystem::with_built_ins` automatically registers the table. Spawning and rendering
   select behavior from the definition rather than species-specific branches. Only a genuinely
   new visual style requires loading another texture and extending `LifeformVisual`.

## Before committing content

Add a uniqueness or behaviour test beside the definition, then run the verification commands in
`AGENTS.md`. If serialized fields change rather than merely adding stable IDs, increment the world
format version and add an explicit migration path.
