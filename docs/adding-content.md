# Adding game content

DeepTek keeps stable IDs and immutable definitions separate from runtime state. Reserve an ID,
add one definition to the appropriate built-in table or registry constructor, add its atlas art,
and only add system code when the content introduces genuinely new behaviour. Never reuse an ID
that may already exist in a saved world.

## Blocks

1. Reserve a foreground `TileId` constant beside `ForegroundTile` in `src/terrain/mod.rs`.
2. Add a `BlockDefinition` to `BUILT_IN_BLOCKS` in `src/terrain/blocks.rs`. This is the shared
   source for mined drops and emitted light.
3. If players can carry or place it, reserve an `ItemId` and add an `ItemDefinition::block` in
   `built_in_item_definitions` (`src/items/registry.rs`). Chain `with_export_value` when it should
   sell for more than the default value of zero.
4. Add the matching tile frame to `assets/terrain/fg_tiles.png`. Foreground tile ID N uses atlas
   frame N; preserve existing frame positions. Background-only tiles use the background atlas and
   do not need foreground gameplay metadata.

Unknown numeric tiles still round-trip through persistence, so staged content does not corrupt a
world. Register a drop before expecting the laser bore to mine that block.

## Furniture and machines

1. Reserve an `ObjectTypeId` in `FurnitureObject` and add one `FurnitureDefinition` to
   `BUILT_IN_FURNITURE` in `src/terrain/furniture.rs`.
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
3. Reserve an item and add `ItemAction::PlaceFurniture` in `built_in_item_definitions`.
   Set its export value on the same definition when appropriate. Add the inventory icon to
   the active item atlas and update `ITEM_ICON_FRAMES`.
4. Add its world frame to the active furniture atlas and update the atlas dimensions in
   `src/terrain_renderer/furniture/mod.rs` when adding a row or column. Furniture with a
   procedural visual should add that visual to the same renderer module instead of reserving an
   unused atlas frame.
5. Put genuinely new behavior in a focused system. Use `World::objects_of_type` for sparse global
   machines, or the scheduled-object path when behavior naturally follows simulation ticks.
   Occupancy, support validation, containers, save/load, placement preview, removal, and the
   interaction window are generic.

An item-transport connector needs no container or update branch: call
`FurnitureDefinition::with_item_transport_connector`. The cargo system discovers all connector
definitions, builds components from O(1) occupancy lookups, and connects them to any adjacent
definition-declared endpoint. Furniture topology has a dedicated revision, so natural object
growth does not trigger network rebuilds.

## Natural decorations

1. Reserve an `ObjectTypeId` in `NaturalObject` and add a `DecorationDefinition` to
   `BUILT_IN_DECORATIONS` in `src/terrain/decorations.rs`.
2. Use `DecorationVisual::Static`, `GrowthFrames`, or `Segmented`; these render without a new
   renderer branch. Add the referenced frames to `assets/decorations/deco.png`.
3. Add generation or active-area spawning rules only if the decoration should appear naturally.
   A decoration with custom growth behaviour also needs a scheduled-update branch.

## Enemies

1. Reserve a `LifeformId` in `src/entity/lifeform.rs`.
2. Construct a `LifeformDefinition` in `built_in_lifeform_definitions`, or register it from a game
   bootstrap with `LifeformSystem::register`.
3. Provide its sprite material/frame and choose where it spawns. The ECS movement, collision,
   health, rendering, and definition validation are shared by every registered lifeform.

## Before committing content

Add a uniqueness or behaviour test beside the definition, then run the verification commands in
`AGENTS.md`. If serialized fields change rather than merely adding stable IDs, increment the world
format version and add an explicit migration path.
