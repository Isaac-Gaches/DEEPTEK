use super::{Preset, Scenario, TimingSummary, measure};
use deep_tek::{
    BackgroundTile, CHUNK_SIZE, ChunkMeshData, ChunkPos, Collider, ForegroundTile, FurnitureObject,
    Layer, LifeformDefinition, LifeformId, LifeformSimulation, LifeformSimulationConfig,
    LifeformSpawnView, LifeformSystem, NatureSimulationConfig, PhysicsConfig, PowerSystem, TilePos,
    Transform, World, build_chunk_mesh_into, prepare_lighting_window, update_colliders,
    update_lighting_window_cells,
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::World as EntityWorld;
use std::hint::black_box;
use std::marker::PhantomData;
use std::time::Duration;

const FRAME_TIME: Duration = Duration::from_nanos(16_666_667);
const DAYTIME: f32 = 0.5;

pub(super) fn run_scenario(scenario: Scenario, preset: Preset) -> TimingSummary {
    match scenario {
        Scenario::EntityPhysics => benchmark_entity_physics(preset),
        Scenario::LifeformAi => benchmark_lifeform_ai(preset),
        Scenario::MachineryLifeforms => benchmark_machinery_lifeforms(preset),
        Scenario::ActiveChunkRefresh => benchmark_active_chunks(preset),
        Scenario::PowerTopologyRebuild => benchmark_power_rebuild(preset),
        Scenario::PowerLocalizedEdit => benchmark_power_local_edit(preset),
        Scenario::PowerDistribution => benchmark_power_distribution(preset),
        Scenario::DrillTick => benchmark_drill_tick(preset),
        Scenario::LightingInput => benchmark_lighting_input(preset),
        Scenario::LightingInputMedium => benchmark_lighting_input_medium(preset),
        Scenario::LightingLocalizedEdit => benchmark_lighting_localized_edit(preset),
        Scenario::TerrainChunkMesh => benchmark_terrain_chunk_mesh(preset),
        Scenario::TerrainEdits => benchmark_terrain_edits(preset),
        Scenario::CombinedFrame => benchmark_combined_frame(preset),
    }
}

fn benchmark_entity_physics(preset: Preset) -> TimingSummary {
    let terrain = floor_world(512, 128, 96);
    let mut entities = EntityWorld::new();
    spawn_colliders(&mut entities, preset.entities, 512, 96);
    measure(
        Scenario::EntityPhysics,
        format!("{} moving colliders", preset.entities),
        preset,
        || {
            update_colliders(
                &mut entities,
                &terrain,
                FRAME_TIME.as_secs_f32(),
                PhysicsConfig::default(),
            );
        },
    )
}

fn benchmark_lifeform_ai(preset: Preset) -> TimingSummary {
    let terrain = floor_world(512, 128, 96);
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = entities.spawn((Transform::new([256.0, 94.0]), Collider::new(1.0, 2.0)));
    for index in 0..preset.entities {
        let position = collider_position(index, 512, 96);
        system
            .spawn(
                &mut entities,
                LifeformId::WALKER,
                benchmark_material(),
                position,
            )
            .expect("the built-in walker is registered");
    }
    measure(
        Scenario::LifeformAi,
        format!("{} lifeforms", preset.entities),
        preset,
        || {
            system.update(&mut entities, target, FRAME_TIME.as_secs_f32());
            update_colliders(
                &mut entities,
                &terrain,
                FRAME_TIME.as_secs_f32(),
                PhysicsConfig::default(),
            );
        },
    )
}

fn benchmark_machinery_lifeforms(preset: Preset) -> TimingSummary {
    let mut fixture = drill_world(preset.drills);
    let system = benchmark_lifeform_system();
    let (mut entities, target) =
        lifeforms_across_world(&system, preset.entities, fixture.world.width());
    let mut config = dormant_spawn_config(preset.entities);
    config.machinery_chunk_radius = [0, 0];
    config.machinery_refresh_interval = Duration::from_millis(250);
    let mut simulation = LifeformSimulation::new(config);
    let view = LifeformSpawnView::new([16.0, 6.0], [0.0, 0.0], [64.0, 64.0]);
    prime_lifeform_simulation(
        &mut simulation,
        &system,
        &mut entities,
        target,
        &mut fixture.world,
        view,
    );
    measure(
        Scenario::MachineryLifeforms,
        format!(
            "{} lifeforms + {} machine targets",
            preset.entities, preset.drills
        ),
        preset,
        || {
            simulation.update(
                &system,
                &mut entities,
                target,
                &mut fixture.world,
                benchmark_material(),
                view,
                FRAME_TIME,
                PhysicsConfig::default(),
            )
        },
    )
}

fn benchmark_active_chunks(preset: Preset) -> TimingSummary {
    let mut terrain = active_chunk_world(preset.active_chunks);
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = entities.spawn((Transform::new([16.0, 16.0]), Collider::new(1.0, 2.0)));
    let mut config = dormant_spawn_config(0);
    config.machinery_refresh_interval = Duration::from_nanos(1);
    let mut simulation = LifeformSimulation::new(config);
    let view = LifeformSpawnView::new([16.0, 16.0], [0.0, 0.0], [32.0, 32.0]);
    measure(
        Scenario::ActiveChunkRefresh,
        format!("{} occupied machinery chunks", preset.active_chunks),
        preset,
        || {
            simulation.update(
                &system,
                &mut entities,
                target,
                &mut terrain,
                benchmark_material(),
                view,
                FRAME_TIME,
                PhysicsConfig::default(),
            )
        },
    )
}

fn benchmark_power_rebuild(preset: Preset) -> TimingSummary {
    let fixture = drill_world(preset.drills);
    let mut probe = PowerSystem::new();
    let update = probe.update(&fixture.world);
    measure(
        Scenario::PowerTopologyRebuild,
        format!(
            "{} nodes / {} candidate links",
            update.node_count, update.candidate_connection_count
        ),
        preset,
        || {
            let mut power = PowerSystem::new();
            power.update(&fixture.world)
        },
    )
}

fn benchmark_power_distribution(preset: Preset) -> TimingSummary {
    let mut fixture = drill_world(preset.drills);
    let mut power = PowerSystem::new();
    power.update(&fixture.world);
    measure(
        Scenario::PowerDistribution,
        format!("{} powered drills", preset.drills),
        preset,
        || power.distribute(&mut fixture.world, DAYTIME, FRAME_TIME),
    )
}

fn benchmark_power_local_edit(preset: Preset) -> TimingSummary {
    let mut fixture = drill_world(preset.drills);
    let mut power = PowerSystem::new();
    let initial = power.update(&fixture.world);
    let scale = format!(
        "one edit in {} nodes / {} candidates",
        initial.node_count, initial.candidate_connection_count
    );
    let anchor = TilePos::new(fixture.world.width() - 2, 6);
    let mut extra_pylon = None;
    measure(Scenario::PowerLocalizedEdit, scale, preset, || {
        if let Some(pylon) = extra_pylon.take() {
            fixture
                .world
                .remove_object(pylon)
                .expect("benchmark pylon remains removable");
        } else {
            extra_pylon = Some(
                fixture
                    .world
                    .place_furniture(FurnitureObject::PYLON, anchor)
                    .expect("benchmark pylon footprint is free"),
            );
        }
        let update = power.update(&fixture.world);
        debug_assert!(!update.full_topology_rebuild);
        update
    })
}

fn benchmark_drill_tick(preset: Preset) -> TimingSummary {
    let mut fixture = drill_world(preset.drills);
    let mut power = PowerSystem::new();
    power.distribute(&mut fixture.world, DAYTIME, Duration::from_secs(1));
    let config = NatureSimulationConfig {
        columns_per_tick: 0,
        max_columns_per_update: 0,
        object_update_budget: preset.drills.saturating_mul(2),
        ..NatureSimulationConfig::default()
    };
    measure(
        Scenario::DrillTick,
        format!("{} simultaneously scheduled drills", preset.drills),
        preset,
        || {
            fixture.world.update_nature_with_power(
                Duration::from_secs(1),
                TilePos::new(1, 1),
                config,
                &power,
            )
        },
    )
}

fn benchmark_lighting_input(preset: Preset) -> TimingSummary {
    const HORIZONTAL_RADIUS: u32 = 3;
    const VERTICAL_RADIUS: u32 = 2;
    let world = representative_terrain(512, 384);
    let mut occupancy = Vec::new();
    let mut lights = Vec::new();
    measure(
        Scenario::LightingInput,
        "256x192 tiles (High preset)".to_owned(),
        preset,
        || {
            prepare_lighting_window(
                &world,
                [256.0, 192.0],
                HORIZONTAL_RADIUS,
                VERTICAL_RADIUS,
                &mut occupancy,
                &mut lights,
            );
            (occupancy.len(), lights.len())
        },
    )
}

fn benchmark_lighting_input_medium(preset: Preset) -> TimingSummary {
    const HORIZONTAL_RADIUS: u32 = 2;
    const VERTICAL_RADIUS: u32 = 1;
    let world = representative_terrain(512, 384);
    let mut occupancy = Vec::new();
    let mut lights = Vec::new();
    measure(
        Scenario::LightingInputMedium,
        "192x128 tiles (Medium preset)".to_owned(),
        preset,
        || {
            prepare_lighting_window(
                &world,
                [256.0, 192.0],
                HORIZONTAL_RADIUS,
                VERTICAL_RADIUS,
                &mut occupancy,
                &mut lights,
            );
            (occupancy.len(), lights.len())
        },
    )
}

fn benchmark_lighting_localized_edit(preset: Preset) -> TimingSummary {
    const HORIZONTAL_RADIUS: u32 = 3;
    const VERTICAL_RADIUS: u32 = 2;
    let mut world = representative_terrain(512, 384);
    let anchor = [256.0, 192.0];
    let changed = [TilePos::new(256, 192)];
    let mut occupancy = Vec::new();
    let mut lights = Vec::new();
    prepare_lighting_window(
        &world,
        anchor,
        HORIZONTAL_RADIUS,
        VERTICAL_RADIUS,
        &mut occupancy,
        &mut lights,
    );
    let mut use_dirt = true;
    measure(
        Scenario::LightingLocalizedEdit,
        "one edited cell in a 256x192 window".to_owned(),
        preset,
        || {
            world
                .set_tile(
                    changed[0].x,
                    changed[0].y,
                    Layer::Foreground,
                    if use_dirt {
                        ForegroundTile::DIRT
                    } else {
                        ForegroundTile::STONE
                    },
                )
                .expect("benchmark edit is in bounds");
            use_dirt = !use_dirt;
            update_lighting_window_cells(
                &world,
                anchor,
                HORIZONTAL_RADIUS,
                VERTICAL_RADIUS,
                &changed,
                &mut occupancy,
                &mut lights,
            )
        },
    )
}

fn benchmark_terrain_chunk_mesh(preset: Preset) -> TimingSummary {
    let world = representative_terrain(CHUNK_SIZE as u32, CHUNK_SIZE as u32);
    let mut mesh = ChunkMeshData::default();
    measure(
        Scenario::TerrainChunkMesh,
        format!("one {CHUNK_SIZE}x{CHUNK_SIZE} foreground layer"),
        preset,
        || {
            build_chunk_mesh_into(
                &world,
                ChunkPos { x: 0, y: 0 },
                Layer::Foreground,
                &mut mesh,
            );
            (mesh.vertices.len(), mesh.indices.len())
        },
    )
}

fn benchmark_terrain_edits(preset: Preset) -> TimingSummary {
    const EDITS: u32 = 256;
    let mut world = representative_terrain(512, 384);
    let mut use_dirt = true;
    measure(
        Scenario::TerrainEdits,
        format!("{EDITS} foreground edits across four chunks"),
        preset,
        || {
            let tile = if use_dirt {
                ForegroundTile::DIRT
            } else {
                ForegroundTile::STONE
            };
            for edit in 0..EDITS {
                let x = 96 + edit % 128;
                let y = 128 + edit / 128;
                world
                    .set_tile(x, y, Layer::Foreground, tile)
                    .expect("benchmark edit is in bounds");
            }
            use_dirt = !use_dirt;
        },
    )
}

fn benchmark_combined_frame(preset: Preset) -> TimingSummary {
    let mut fixture = drill_world(preset.drills);
    let mut power = PowerSystem::new();
    power.distribute(&mut fixture.world, DAYTIME, FRAME_TIME);
    let system = benchmark_lifeform_system();
    let (mut entities, target) =
        lifeforms_across_world(&system, preset.entities, fixture.world.width());
    let mut config = dormant_spawn_config(preset.entities);
    config.machinery_chunk_radius = [0, 0];
    config.machinery_refresh_interval = Duration::from_millis(250);
    let mut simulation = LifeformSimulation::new(config);
    let view = LifeformSpawnView::new([16.0, 6.0], [0.0, 0.0], [64.0, 64.0]);
    let nature = NatureSimulationConfig {
        object_update_budget: preset.drills.saturating_mul(2),
        ..NatureSimulationConfig::default()
    };
    prime_lifeform_simulation(
        &mut simulation,
        &system,
        &mut entities,
        target,
        &mut fixture.world,
        view,
    );
    measure(
        Scenario::CombinedFrame,
        format!("{} lifeforms + {} drills", preset.entities, preset.drills),
        preset,
        || {
            let flow = power.distribute(&mut fixture.world, DAYTIME, FRAME_TIME);
            let lifeforms = simulation.update(
                &system,
                &mut entities,
                target,
                &mut fixture.world,
                benchmark_material(),
                view,
                FRAME_TIME,
                PhysicsConfig::default(),
            );
            let nature = fixture.world.update_nature_with_power(
                FRAME_TIME,
                TilePos::new(16, 6),
                nature,
                &power,
            );
            (flow, lifeforms, nature)
        },
    )
}

fn floor_world(width: u32, height: u32, floor_y: u32) -> World {
    let mut world = World::empty(width, height, 0x0D33_F7E4).expect("valid benchmark world");
    for x in 0..width {
        world
            .set_tile(x, floor_y, Layer::Foreground, ForegroundTile::STONE)
            .expect("floor coordinate is in bounds");
    }
    world
}

fn representative_terrain(width: u32, height: u32) -> World {
    let mut world = World::empty(width, height, 0x11A4_71A6).expect("valid benchmark world");
    for y in 0..height {
        for x in 0..width {
            if y > height / 3 && !(x / 9 + y / 7).is_multiple_of(5) {
                world
                    .set_tile(x, y, Layer::Foreground, ForegroundTile::STONE)
                    .expect("representative foreground coordinate is in bounds");
            } else if y > height / 4 {
                world
                    .set_tile(x, y, Layer::Background, BackgroundTile::STONE_WALL)
                    .expect("representative background coordinate is in bounds");
            }
            if y > height / 3 && (x + y * 3).is_multiple_of(257) {
                world
                    .set_tile(x, y, Layer::Foreground, ForegroundTile::ASTERITE)
                    .expect("representative light coordinate is in bounds");
            }
        }
    }
    world
}

fn spawn_colliders(entities: &mut EntityWorld, count: usize, width: u32, floor_y: u32) {
    for index in 0..count {
        entities.spawn((
            Transform::new(collider_position(index, width, floor_y)),
            Collider::new(0.9, 1.7)
                .with_velocity([if index.is_multiple_of(2) { 8.0 } else { -8.0 }, 2.0])
                .with_material(0.0, 0.2),
        ));
    }
}

fn collider_position(index: usize, width: u32, floor_y: u32) -> [f32; 2] {
    let columns = width.saturating_sub(4) as usize;
    let x = 2.0 + (index % columns) as f32;
    let row = (index / columns) % 40;
    [x, floor_y as f32 - 2.0 - row as f32 * 1.8]
}

fn active_chunk_world(count: usize) -> World {
    let columns = 50_usize.min(count.max(1));
    let rows = count.div_ceil(columns);
    let width = (columns * 64) as u32;
    let height = (rows * 64) as u32;
    let mut world = World::empty(width, height, 0x00AC_71CE).expect("valid active-chunk world");
    for index in 0..count {
        let chunk_x = index % columns;
        let chunk_y = index / columns;
        world
            .place_furniture(
                FurnitureObject::CARGO_CONVEYOR,
                TilePos::new(
                    (chunk_x * deep_tek::CHUNK_SIZE + 8) as u32,
                    (chunk_y * deep_tek::CHUNK_SIZE + 8) as u32,
                ),
            )
            .expect("one free-standing conveyor fits in each chunk");
    }
    world
}

struct DrillFixture {
    world: World,
}

fn drill_world(drills: usize) -> DrillFixture {
    const STRIDE: u32 = 9;
    const FLOOR_Y: u32 = 8;
    const HEIGHT: u32 = 192;
    let width = drills as u32 * STRIDE + 2;
    let mut world = floor_world(width, HEIGHT, FLOOR_Y);
    for index in 0..drills {
        let x = index as u32 * STRIDE;
        for y in 12..HEIGHT {
            world
                .set_tile(x + 1, y, Layer::Foreground, ForegroundTile::STONE)
                .expect("drill target is in bounds");
        }
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(x, 5))
            .expect("bore support and footprint are valid");
        assert!(world.set_furniture_active(bore, true));
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(x + 4, 5))
            .expect("solar array support and footprint are valid");
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(x + 7, 6))
            .expect("pylon support and footprint are valid");
    }
    DrillFixture { world }
}

fn dormant_spawn_config(maximum_lifeforms: usize) -> LifeformSimulationConfig {
    LifeformSimulationConfig {
        player_spawn_interval: Duration::from_secs(3_600),
        machinery_spawn_interval: Duration::from_secs(3_600),
        maximum_lifeforms,
        ..LifeformSimulationConfig::default()
    }
}

fn benchmark_lifeform_system() -> LifeformSystem {
    let mut definition = LifeformDefinition::walker(LifeformId::WALKER, "benchmark walker");
    definition.attack_damage = 1;
    definition.attack_interval = 3_600.0;
    let mut system = LifeformSystem::new();
    system
        .register(definition)
        .expect("benchmark lifeform definition is valid");
    system
}

fn lifeforms_across_world(
    system: &LifeformSystem,
    count: usize,
    width: u32,
) -> (EntityWorld, hecs::Entity) {
    let mut entities = EntityWorld::new();
    let target = entities.spawn((Transform::new([16.0, 6.0]), Collider::new(1.0, 2.0)));
    for index in 0..count {
        let x = 1.0 + (index % width.saturating_sub(2) as usize) as f32;
        system
            .spawn(
                &mut entities,
                LifeformId::WALKER,
                benchmark_material(),
                [x, 6.0],
            )
            .expect("the benchmark walker is registered");
    }
    (entities, target)
}

fn prime_lifeform_simulation(
    simulation: &mut LifeformSimulation,
    system: &LifeformSystem,
    entities: &mut EntityWorld,
    target: hecs::Entity,
    terrain: &mut World,
    view: LifeformSpawnView,
) {
    black_box(simulation.update(
        system,
        entities,
        target,
        terrain,
        benchmark_material(),
        view,
        Duration::from_millis(250),
        PhysicsConfig::default(),
    ));
}

fn benchmark_material() -> Handle<Material> {
    Handle {
        index: 0,
        generation: 0,
        _marker: PhantomData,
    }
}
