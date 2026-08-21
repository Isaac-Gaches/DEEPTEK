//! Chunked terrain, ECS-driven entities, physics, and GPU rendering.

pub mod contracts;
pub mod entity;
pub mod entity_renderer;
pub mod gui;
pub mod item_transport;
pub mod items;
pub mod orbital_export;
pub mod post_process;
pub mod power;
mod render_common;
pub mod sky_renderer;
pub mod terrain;
pub mod terrain_renderer;

pub use contracts::{
    Contract, ContractCompany, ContractExportResult, ExportContractProgress,
    apply_export_to_contracts, built_in_contracts,
};
pub use entity::{
    Bomb, Collider, DynamicLight, EffectsMaterials, EffectsSystem, Energy, FollowCamera, Health,
    Lifeform, LifeformDefinition, LifeformId, LifeformSystem, LifeformSystemError, Lifetime,
    Particle, ParticleKind, PhysicsConfig, Player, PlayerInput, Projectile, ProjectileSystem,
    Sprite, Transform, TurretProjectile, TurretStats, TurretSystem, Wallet,
    built_in_lifeform_definitions, entity_position, spawn_bomb, spawn_glowstick, spawn_player,
    update_colliders, update_player_animation, update_players,
};
pub use entity_renderer::{SpriteAtlasFrame, SpriteInstance, SpriteRenderer};
pub use gui::{
    BatteryStatus, CargoLiftStatus, ContractsAction, ContractsGui, FurnitureControlAction,
    FurnitureGuiState, GuiRenderer, HudAction, HudGui, HudSnapshot, InventoryGui, MeterValue,
    WorldMapGui,
};
pub use item_transport::{
    DEFAULT_ITEM_TRANSPORT_INTERVAL, ItemTransportShape, ItemTransportSystem, ItemTransportUpdate,
    item_transport_shape,
};
pub use items::{
    CHEST_COLUMNS, CHEST_ROWS, CHEST_SLOTS, ConsumableAction, DEFAULT_ITEM_REACH, HOTBAR_SLOTS,
    INVENTORY_COLUMNS, INVENTORY_ROWS, INVENTORY_SLOTS, Inventory, ItemAction, ItemCategory,
    ItemContainer, ItemDefinition, ItemId, ItemRegistry, ItemRegistryError, ItemStack,
    ItemTargetStatus, ItemUseResult, ProjectileKind, SlotClick, ToolAction,
    built_in_item_definitions, selected_item_target_size, selected_item_target_status,
    use_selected_item,
};
pub use orbital_export::{DEFAULT_ORBITAL_EXPORT_INTERVAL, ExportShipment, OrbitalExportSystem};
pub use post_process::BloomRenderer;
pub use power::{
    MACHINE_CONNECTION_LIMIT, POWER_CONNECTOR_CONNECTION_LIMIT, PYLON_CONNECTION_LIMIT,
    PowerConnection, PowerFlow, PowerSystem, PowerUpdate, is_daytime,
};
pub use sky_renderer::{SkyRenderConfig, SkyRenderer};
pub use terrain::{
    BATTERY_CAPACITY_MILLI, BATTERY_DEFINITION, BUILT_IN_BLOCKS, BUILT_IN_DECORATIONS,
    BUILT_IN_FURNITURE, BackgroundTile, BlockDefinition, CARGO_CONVEYOR_DEFINITION,
    CARGO_LIFT_DEFINITION, CARGO_LIFT_DEMAND_MILLI_PER_SECOND, CARGO_LIFT_SLOTS,
    CARGO_LIFT_SPEED_MILLI_TILES_PER_SECOND, CHEST_DEFINITION, CHUNK_SIZE, CargoLiftDirection,
    Chunk, ChunkPos, DecorationDefinition, DecorationUpdate, DecorationVisual, ForegroundTile,
    FurnitureConfiguration, FurnitureDefinition, FurnitureInteraction, FurnitureObject,
    FurnitureSupport, ItemTransportRole, LASER_BORE_DEFINITION, LASER_BORE_DEMAND_MILLI_PER_SECOND,
    LASER_BORE_MAX_LENGTH, LASER_BORE_SLOTS, LASER_BORE_TICKS_PER_TILE, LIFT_STATION_DEFINITION,
    LIFT_STATION_SLOTS, Layer, LiftStationConfiguration, LiftStationMode, MAX_VINE_LENGTH,
    MAX_WORLD_HEIGHT, MAX_WORLD_NAME_BYTES, MAX_WORLD_TILES, MAX_WORLD_WIDTH, METRES_PER_TILE,
    NaturalObject, NatureSimulationConfig, NatureUpdate, ORBITAL_EXPORT_DEMAND_MILLI_PER_SECOND,
    ORBITAL_EXPORT_LAUNCHER_DEFINITION, ORBITAL_EXPORT_LAUNCHER_SLOTS, ObjectId,
    ObjectPlacementError, ObjectTypeId, POWER_CONNECTION_RANGE_HALF_TILES,
    POWER_CONNECTION_RANGE_TILES, POWER_CONNECTOR_DEFINITION, POWER_CONNECTOR_RANGE_TILES,
    POWERED_CABLE_ANCHOR_DEFINITION, POWERED_CABLE_OBJECT, PYLON_DEFINITION, PowerRole,
    ROPE_OBJECT, SEA_LEVEL_PERCENT, SOLAR_ARRAY_DEFINITION, SOLAR_GENERATION_MILLI_PER_SECOND,
    TURRET_DEFINITION, TURRET_DEMAND_MILLI_PER_SECOND, TargetPriority, TileId, TilePos, World,
    WorldError, WorldGenerator, WorldObject, block_definition, decoration_definition,
    furniture_definition,
};
pub use terrain_renderer::{
    LightSource, LightingUpdateStats, MeshSyncStats, TerrainRenderConfig, TerrainRenderer,
    TerrainVertex,
};
