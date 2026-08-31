//! Chunked terrain, ECS-driven entities, physics, and GPU rendering.

pub mod contracts;
pub mod delivery;
pub mod entity;
pub mod entity_renderer;
pub mod gui;
pub mod item_transport;
pub mod items;
pub mod machine_processing;
pub mod orbital_export;
pub mod post_process;
pub mod power;
mod render_common;
pub mod sky_renderer;
pub mod specialists;
pub mod terrain;
pub mod terrain_renderer;
pub mod transmissions;
pub mod tutorial;

pub use contracts::{
    AcceptContractError, BuildAndExportContractProgress, CORPORATION_LEVEL_THRESHOLDS,
    ClaimContractError, Contract, ContractBoard, ContractCompany, ContractExportResult, ContractId,
    ContractProgressResult, ContractReward, CorporationProgress, ExportContractProgress,
    MAX_ACTIVE_CONTRACTS, MAX_CORPORATION_LEVEL, MineContractProgress, apply_export_to_contracts,
    apply_mined_to_contracts, built_in_contracts,
};
pub use delivery::{
    DELIVERY_DELAY_SECONDS, DELIVERY_DROP_HEIGHT, DeliveryCrate, DeliverySystem, MACHINE_OFFERS,
    MachineOffer, PROGRAM_DELIVERY_DELAY_SECONDS, PurchaseError, ScheduleDeliveryError,
    machine_offer, spawn_delivery_crate,
};
pub use entity::{
    Bomb, Collider, DynamicLight, EffectsMaterials, EffectsSystem, Energy, FollowCamera,
    GLOWGNAT_MIN_MACHINERY_ATTENTION, Health, Lifeform, LifeformDefinition, LifeformId,
    LifeformLocomotion, LifeformMaterials, LifeformSimulation, LifeformSimulationConfig,
    LifeformSimulationUpdate, LifeformSpawnView, LifeformSystem, LifeformSystemError, Lifetime,
    MAX_TURRET_ELEVATION_DEGREES, Particle, ParticleKind, PhysicsConfig, Player, PlayerInput,
    Projectile, ProjectileSystem, SPIKE_CONTACT_DAMAGE, SPIKE_DAMAGE_INTERVAL_SECONDS,
    SpikeDamageSystem, SpikeDamageUpdate, Sprite, Transform, TurretProjectile, TurretStats,
    TurretSystem, Wallet, built_in_lifeform_definitions, entity_position, spawn_bomb,
    spawn_glowstick, spawn_player, update_colliders, update_player_animation, update_players,
};
pub use entity_renderer::{SpriteAtlasFrame, SpriteInstance, SpriteRenderer};
pub use gui::{
    BatteryStatus, CargoLiftStatus, ContractsAction, ContractsGui, ContractsTab,
    FurnitureControlAction, FurnitureGuiState, GuiRenderer, HudAction, HudGui, HudSnapshot,
    InventoryGui, MeterValue, ProcurementAction, ProcurementGui, ProcurementView, SpecialistAction,
    SpecialistGui, SpecialistView, SubsurfaceSurveyStatus, WorldMapGui,
    handle_incoming_transmission_click, incoming_transmission_captures_pointer,
    queue_incoming_transmission,
};
pub use item_transport::{
    DEFAULT_ITEM_TRANSPORT_INTERVAL, ItemTransportShape, ItemTransportSystem, ItemTransportUpdate,
    item_transport_shape,
};
pub use items::{
    CHEST_COLUMNS, CHEST_ROWS, CHEST_SLOTS, CRAFTING_RECIPES, ConsumableAction, CraftingError,
    CraftingIngredient, CraftingRecipe, DEFAULT_ITEM_REACH, DROPPED_ITEM_ICON_FRAMES, DroppedItem,
    DroppedItemContext, DroppedItemSystem, DroppedItemUpdate, HOTBAR_SLOTS, INVENTORY_COLUMNS,
    INVENTORY_ROWS, INVENTORY_SLOTS, Inventory, ItemAction, ItemCategory, ItemContainer,
    ItemDefinition, ItemId, ItemRegistry, ItemRegistryError, ItemStack, ItemTargetStatus,
    ItemUseResult, ProjectileKind, SlotClick, ToolAction, built_in_item_definitions,
    crafting_recipe, selected_item_target_size, selected_item_target_status, use_selected_item,
    use_selected_item_in_background,
};
pub use machine_processing::{
    COMPOSITE_ASSEMBLY_INTERVAL, COMPOSITE_RECIPE, MachineProcessingSystem,
    MachineProcessingUpdate, ProcessingRecipe,
};
pub use orbital_export::{DEFAULT_ORBITAL_EXPORT_INTERVAL, ExportShipment, OrbitalExportSystem};
pub use post_process::BloomRenderer;
pub use power::{
    MACHINE_CONNECTION_LIMIT, POWER_CONNECTOR_CONNECTION_LIMIT, PYLON_CONNECTION_LIMIT,
    PowerConnection, PowerFlow, PowerSystem, PowerUpdate, is_daytime,
};
pub use sky_renderer::{SkyRenderConfig, SkyRenderer};
pub use specialists::{
    BUILT_IN_SPECIALISTS, HappinessFactor, HappinessReport, HappinessRule, HouseRequirements,
    MAX_HOUSE_INTERIOR_CELLS, MIN_HOUSE_INTERIOR_CELLS, RecruitSpecialistError, RoomAssessment,
    Specialist, SpecialistBonus, SpecialistBonuses, SpecialistDefinition, SpecialistId,
    SpecialistOfferState, SpecialistRecord, SpecialistSystem, SpecialistTerminalView, assess_bed,
    assess_room, happiness_report, specialist_definition,
};
pub use terrain::{
    AMMO_TURRET_DEFINITION, AMMO_TURRET_DEMAND_MILLI_PER_SECOND, AMMO_TURRET_SLOTS,
    BATTERY_CAPACITY_MILLI, BATTERY_DEFINITION, BED_DEFINITION, BUILT_IN_BLOCKS,
    BUILT_IN_DECORATIONS, BUILT_IN_FURNITURE, BackgroundTile, BiomeId, BiomeMap, BlockDamage,
    BlockDefinition, BlockHealth, BrokenTile, CARGO_CONVEYOR_DEFINITION, CARGO_LIFT_DEFINITION,
    CARGO_LIFT_DEMAND_MILLI_PER_SECOND, CARGO_LIFT_SLOTS, CARGO_LIFT_SPEED_MILLI_TILES_PER_SECOND,
    CHEST_DEFINITION, CHUNK_SIZE, COMPOSITE_ASSEMBLER_DEFINITION,
    COMPOSITE_ASSEMBLER_DEMAND_MILLI_PER_SECOND, COMPOSITE_ASSEMBLER_SLOTS, CargoLiftDirection,
    Chunk, ChunkActivity, ChunkPos, DEFAULT_BLOCK_HEALTH, DEFAULT_MACHINE_HEALTH,
    DIRECTIONAL_SENTRY_DEFINITION, DIRECTIONAL_SENTRY_DEMAND_MILLI_PER_SECOND, DOOR_DEFINITION,
    DecorationDefinition, DecorationUpdate, DecorationVisual, ForegroundTile,
    FurnitureConfiguration, FurnitureDefinition, FurnitureFacing, FurnitureInteraction,
    FurnitureObject, FurnitureSupport, ItemTransportRole, LASER_BORE_DEFINITION,
    LASER_BORE_DEMAND_MILLI_PER_SECOND, LASER_BORE_MAX_LENGTH, LASER_BORE_SLOTS,
    LASER_BORE_TICKS_PER_TILE, LASER_DRILL_DEFINITION, LASER_DRILL_DEMAND_MILLI_PER_SECOND,
    LASER_DRILL_MAX_LENGTH, LASER_DRILL_SLOTS, LIFT_STATION_DEFINITION, LIFT_STATION_SLOTS,
    LaserDrillAim, Layer, LiftStationConfiguration, LiftStationMode, MAX_SURVEY_ORE_TYPES,
    MAX_VINE_LENGTH, MAX_WORLD_HEIGHT, MAX_WORLD_NAME_BYTES, MAX_WORLD_TILES, MAX_WORLD_WIDTH,
    METRES_PER_TILE, MachineDamage, MachineHealth, NaturalObject, NatureSimulationConfig,
    NatureUpdate, ORBITAL_EXPORT_DEMAND_MILLI_PER_SECOND, ORBITAL_EXPORT_LAUNCHER_DEFINITION,
    ORBITAL_EXPORT_LAUNCHER_SLOTS, ObjectId, ObjectPlacementError, ObjectTypeId, OreEstimate,
    POWER_CONNECTION_RANGE_HALF_TILES, POWER_CONNECTION_RANGE_TILES, POWER_CONNECTOR_DEFINITION,
    POWER_CONNECTOR_RANGE_TILES, POWERED_CABLE_ANCHOR_DEFINITION, POWERED_CABLE_OBJECT,
    PROCUREMENT_TERMINAL_DEFINITION, PYLON_DEFINITION, PlayerState, PowerRole,
    RED_SHAFT_BORE_DEFINITION, RED_SHAFT_BORE_DEMAND_MILLI_PER_SECOND, RED_SHAFT_BORE_SLOTS,
    RED_SHAFT_BORE_WIDTH, ROPE_OBJECT, RemovedObject, SEA_LEVEL_PERCENT, SOLAR_ARRAY_DEFINITION,
    SOLAR_GENERATION_MILLI_PER_SECOND, SPIKES_DEFINITION, SUBSURFACE_SURVEY_DEPTH,
    SUBSURFACE_SURVEY_WIDTH, SUBSURFACE_SURVEYOR_DEFINITION,
    SUBSURFACE_SURVEYOR_DEMAND_MILLI_PER_SECOND, SubsurfaceSurvey, TURRET_DEFINITION,
    TURRET_DEMAND_MILLI_PER_SECOND, TargetPriority, TileId, TilePos, World, WorldError,
    WorldGenerator, WorldObject, background_tile_for, block_definition, decoration_definition,
    furniture_definition,
};
pub use terrain_renderer::{
    LightSource, LightingUpdateStats, MeshSyncStats, TerrainRenderConfig, TerrainRenderer,
    TerrainVertex,
};
pub use transmissions::{Transmission, TransmissionLog};
pub use tutorial::{TUTORIAL_BRIEFING_DELAY_SECONDS, TutorialProgram};
