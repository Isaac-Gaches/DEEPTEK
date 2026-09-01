use super::*;
use crate::{ForegroundTile, FurnitureObject, LifeformDefinition, TilePos};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use std::marker::PhantomData;

fn material() -> Handle<Material> {
    Handle {
        index: 0,
        generation: 0,
        _marker: PhantomData,
    }
}

fn floor_world(width: u32) -> World {
    let mut world = World::empty(width, 64, 17).unwrap();
    for x in 0..width {
        world
            .set_tile(x, 40, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
}

fn target(entities: &mut EntityWorld, position: [f32; 2]) -> Entity {
    entities.spawn((Transform::new(position), Collider::new(1.0, 2.0)))
}

fn immediate_config() -> LifeformSimulationConfig {
    LifeformSimulationConfig {
        player_chunk_radius: [0, 0],
        machinery_chunk_radius: [0, 0],
        player_spawn_interval: Duration::from_millis(1),
        machinery_refresh_interval: Duration::from_millis(1),
        machinery_spawn_interval: Duration::from_millis(1),
        max_player_chunks_per_spawn: 1,
        max_machinery_chunks_per_spawn: 1,
        spawn_attempts_per_chunk: 16,
        maximum_lifeforms: 8,
        maximum_lifeforms_per_chunk: 2,
        visibility_margin_tiles: 0,
        ambient_spawn_attention: 1,
        attention_per_lifeform: 1,
        attention_for_guaranteed_spawn: 1,
        minimum_hostile_attention: 1,
        max_block_attacks_per_update: 32,
    }
}

#[test]
fn grounded_lifeform_auto_jumps_a_one_tile_obstacle() {
    let mut terrain = floor_world(16);
    terrain
        .set_tile(5, 39, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let system = LifeformSystem::with_built_ins();
    let mut lifeform = Lifeform::new(LifeformId::WALKER, [4.0, 39.0]);
    let mut transform = Transform::new([4.0, 39.0]);
    let mut collider = Collider {
        on_ground: true,
        ..Collider::new(1.0, 1.0)
    };

    advance_lifeform(
        &system,
        &mut lifeform,
        &mut transform,
        &mut collider,
        [10.0, 39.0],
        [0.0, 0.0],
        true,
        &terrain,
        0.016,
        PhysicsConfig::default(),
    );

    assert!(collider.velocity[1] < 0.0);
    assert!(!collider.on_ground);
    assert!(lifeform.jump_cooldown_remaining > 0.0);
}

#[test]
fn visible_space_never_spawns_lifeforms() {
    let mut terrain = floor_world(64);
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = target(&mut entities, [32.0, 38.0]);
    let mut simulation = LifeformSimulation::new(immediate_config());

    let update = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        LifeformSpawnView::new([32.0, 38.0], [-10.0, -10.0], [80.0, 80.0]),
        Duration::from_millis(1),
        PhysicsConfig::default(),
    );

    assert_eq!(update.spawned, 0);
    assert_eq!(entities.query::<&Lifeform>().iter().count(), 0);
}

#[test]
fn player_active_chunks_spawn_only_outside_visibility() {
    let mut terrain = floor_world(64);
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = target(&mut entities, [32.0, 38.0]);
    let mut simulation = LifeformSimulation::new(immediate_config());
    let view = LifeformSpawnView::new([32.0, 38.0], [0.0, 0.0], [8.0, 12.0]);

    let update = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        view,
        Duration::from_millis(1),
        PhysicsConfig::default(),
    );

    assert!(update.spawned > 0);
    for (_, (transform, collider)) in entities.query::<(&Transform, &Collider)>().iter() {
        if transform.position == [32.0, 38.0] {
            continue;
        }
        assert!(!view.intersects_visible_area(transform.position, collider.half_extents, 0.0));
    }
}

#[test]
fn active_machinery_spawns_in_distant_chunks_at_the_slow_rate() {
    let mut terrain = floor_world(192);
    let bore = terrain
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(130, 37))
        .unwrap();
    assert!(terrain.set_furniture_active(bore, true));
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = target(&mut entities, [16.0, 38.0]);
    let mut config = immediate_config();
    config.player_spawn_interval = Duration::from_secs(60);
    let mut simulation = LifeformSimulation::new(config);

    let update = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        LifeformSpawnView::new([16.0, 38.0], [0.0, 0.0], [63.0, 63.0]),
        Duration::from_millis(1),
        PhysicsConfig::default(),
    );

    assert_eq!(update.active_machinery_chunks, 1);
    assert_eq!(update.machinery_chunks_checked, 1);
    assert!(update.spawned > 0);
    assert!(
        entities
            .query::<&Lifeform>()
            .iter()
            .any(|(_, lifeform)| { lifeform.id == LifeformId::WALKER })
    );
}

#[test]
fn larger_machine_setups_sustain_more_lifeforms_than_small_setups() {
    fn run(bore_positions: &[u32]) -> (LifeformSimulationUpdate, usize) {
        let mut terrain = floor_world(192);
        for &x in bore_positions {
            let bore = terrain
                .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(x, 37))
                .unwrap();
            assert!(terrain.set_furniture_active(bore, true));
        }
        let system = LifeformSystem::with_built_ins();
        let mut entities = EntityWorld::new();
        let target = target(&mut entities, [16.0, 38.0]);
        let mut config = immediate_config();
        config.player_spawn_interval = Duration::from_secs(60);
        config.attention_per_lifeform = 24;
        let mut simulation = LifeformSimulation::new(config);
        let update = simulation.update(
            &system,
            &mut entities,
            target,
            &mut terrain,
            material(),
            LifeformSpawnView::new([16.0, 38.0], [0.0, 0.0], [63.0, 63.0]),
            Duration::from_millis(1),
            PhysicsConfig::default(),
        );
        let population = entities.query::<&Lifeform>().iter().count();
        (update, population)
    }

    let (small, small_population) = run(&[130]);
    let (large, large_population) = run(&[130, 136, 142]);

    assert_eq!(small_population, 1);
    assert_eq!(large_population, 2);
    assert!(large.distant_machine_attention > small.distant_machine_attention);
}

#[test]
fn distant_machinery_lifeforms_update_every_frame_between_refreshes() {
    let mut terrain = floor_world(192);
    let bore = terrain
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(130, 37))
        .unwrap();
    assert!(terrain.set_furniture_active(bore, true));
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = target(&mut entities, [16.0, 38.0]);
    let walker = system
        .spawn(&mut entities, LifeformId::WALKER, material(), [150.0, 38.0])
        .unwrap();
    let mut config = immediate_config();
    config.player_spawn_interval = Duration::from_secs(60);
    config.machinery_spawn_interval = Duration::from_secs(60);
    config.machinery_refresh_interval = Duration::from_millis(100);
    let mut simulation = LifeformSimulation::new(config);
    let view = LifeformSpawnView::new([16.0, 38.0], [0.0, 0.0], [63.0, 63.0]);

    let first = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        view,
        Duration::from_millis(100),
        PhysicsConfig::default(),
    );
    let second = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        view,
        Duration::from_millis(50),
        PhysicsConfig::default(),
    );
    let third = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        view,
        Duration::from_millis(50),
        PhysicsConfig::default(),
    );

    assert_eq!(first.lifeforms_updated_near_machinery, 1);
    assert_eq!(second.lifeforms_updated_near_machinery, 1);
    assert_eq!(third.lifeforms_updated_near_machinery, 1);
    assert!(!entities.get::<&Collider>(walker).unwrap().enabled);
}

#[test]
fn passive_machinery_retains_only_its_occupied_chunk() {
    let mut terrain = floor_world(192);
    terrain
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(130, 38))
        .unwrap();
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = target(&mut entities, [16.0, 38.0]);
    system
        .spawn(&mut entities, LifeformId::WALKER, material(), [150.0, 38.0])
        .unwrap();
    system
        .spawn(&mut entities, LifeformId::WALKER, material(), [90.0, 38.0])
        .unwrap();
    let mut config = immediate_config();
    config.machinery_chunk_radius = [2, 0];
    config.player_spawn_interval = Duration::from_secs(60);
    config.machinery_spawn_interval = Duration::from_secs(60);
    let mut simulation = LifeformSimulation::new(config);

    let update = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        LifeformSpawnView::new([16.0, 38.0], [0.0, 0.0], [63.0, 63.0]),
        Duration::from_millis(1),
        PhysicsConfig::default(),
    );

    assert_eq!(update.active_machinery_chunks, 1);
    assert_eq!(update.lifeforms_updated_near_machinery, 1);
    assert_eq!(update.distant_machine_attention, 0);
}

#[test]
fn lifeforms_prioritize_and_disable_active_machines() {
    let mut terrain = floor_world(128);
    let bore = terrain
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(64, 37))
        .unwrap();
    terrain
        .container_mut(bore)
        .unwrap()
        .set_slot(0, crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 12));
    assert!(terrain.set_furniture_active(bore, true));
    let mut system = LifeformSystem::new();
    let mut walker_definition = LifeformDefinition::walker(LifeformId::WALKER, "Saboteur");
    walker_definition.attack_damage = crate::DEFAULT_MACHINE_HEALTH;
    system.register(walker_definition).unwrap();
    let mut entities = EntityWorld::new();
    let player = target(&mut entities, [80.0, 38.0]);
    let walker = system
        .spawn(&mut entities, LifeformId::WALKER, material(), [67.0, 38.0])
        .unwrap();
    let mut config = immediate_config();
    config.player_spawn_interval = Duration::from_secs(60);
    config.machinery_spawn_interval = Duration::from_secs(60);
    let mut simulation = LifeformSimulation::new(config);

    let update = simulation.update(
        &system,
        &mut entities,
        player,
        &mut terrain,
        material(),
        LifeformSpawnView::new([80.0, 38.0], [64.0, 30.0], [96.0, 50.0]),
        Duration::from_millis(1),
        PhysicsConfig::default(),
    );

    assert!(entities.get::<&Collider>(walker).unwrap().velocity[0] < 0.0);
    assert_eq!(update.machine_attacks, 1);
    assert_eq!(
        update.machine_damage,
        u32::from(crate::DEFAULT_MACHINE_HEALTH)
    );
    assert_eq!(update.machines_disabled, 1);
    assert_eq!(
        update.machine_contents_dropped,
        vec![(
            TilePos::new(64, 37),
            crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 12).unwrap()
        )]
    );
    assert!(terrain.container(bore).unwrap().is_empty());
    assert!(terrain.machine_health(bore).unwrap().is_disabled());
    assert!(!terrain.object(bore).unwrap().is_active());
    assert!(!terrain.set_furniture_active(bore, true));
}

#[test]
fn lifeforms_chip_a_block_only_when_a_loud_machine_is_obstructed() {
    fn run(object_type: crate::ObjectTypeId) -> (LifeformSimulationUpdate, World) {
        let mut terrain = floor_world(128);
        for y in 37..=39 {
            terrain
                .set_tile(69, y, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let machine = terrain
            .place_furniture(object_type, TilePos::new(72, 37))
            .unwrap();
        terrain.set_furniture_active(machine, true);
        let system = LifeformSystem::with_built_ins();
        let mut entities = EntityWorld::new();
        let player = target(&mut entities, [100.0, 38.0]);
        system
            .spawn(&mut entities, LifeformId::WALKER, material(), [68.0, 38.0])
            .unwrap();
        assert_eq!(
            blocking_block_towards(&terrain, [68.0, 38.0], [0.55, 0.85], [73.0, 38.0]),
            Some(TilePos::new(69, 38))
        );
        let mut config = immediate_config();
        config.player_spawn_interval = Duration::from_secs(60);
        config.machinery_spawn_interval = Duration::from_secs(60);
        let mut simulation = LifeformSimulation::new(config);
        let mut update = LifeformSimulationUpdate::default();
        for _ in 0..=300 {
            update = simulation.update(
                &system,
                &mut entities,
                player,
                &mut terrain,
                material(),
                LifeformSpawnView::new([100.0, 38.0], [90.0, 30.0], [110.0, 48.0]),
                Duration::from_millis(50),
                PhysicsConfig::default(),
            );
            if update.block_attacks > 0 {
                break;
            }
        }
        (update, terrain)
    }

    let (loud, loud_world) = run(FurnitureObject::LASER_BORE);
    assert_eq!(loud.block_attacks, 1);
    assert_eq!(loud.block_damage, 4);
    assert_eq!(
        loud_world
            .block_health(TilePos::new(69, 38), Layer::Foreground)
            .unwrap()
            .unwrap()
            .current(),
        36
    );

    let (quiet, quiet_world) = run(FurnitureObject::SOLAR_ARRAY);
    assert_eq!(quiet.block_attacks, 0);
    assert_eq!(quiet_world.damaged_block_count(), 0);
}

#[test]
fn lifeform_block_attacks_are_globally_budgeted_and_report_breaks() {
    let mut terrain = floor_world(128);
    for y in 37..=39 {
        terrain
            .set_tile(69, y, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bore = terrain
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(72, 37))
        .unwrap();
    assert!(terrain.set_furniture_active(bore, true));
    let mut system = LifeformSystem::new();
    let mut definition = LifeformDefinition::walker(LifeformId::WALKER, "Tunneller");
    definition.block_attack_damage = 40;
    system.register(definition).unwrap();
    let mut entities = EntityWorld::new();
    let player = target(&mut entities, [100.0, 38.0]);
    for y in [37.4, 38.0, 38.6] {
        system
            .spawn(&mut entities, LifeformId::WALKER, material(), [68.0, y])
            .unwrap();
    }
    let mut config = immediate_config();
    config.player_spawn_interval = Duration::from_secs(60);
    config.machinery_spawn_interval = Duration::from_secs(60);
    config.max_block_attacks_per_update = 1;
    let mut simulation = LifeformSimulation::new(config);

    let mut update = LifeformSimulationUpdate::default();
    for _ in 0..=100 {
        update = simulation.update(
            &system,
            &mut entities,
            player,
            &mut terrain,
            material(),
            LifeformSpawnView::new([100.0, 38.0], [90.0, 30.0], [110.0, 48.0]),
            Duration::from_millis(50),
            PhysicsConfig::default(),
        );
        if update.block_attacks > 0 {
            break;
        }
    }

    assert_eq!(update.block_attacks, 1);
    assert_eq!(update.blocks_broken.len(), 1);
    assert_eq!(update.blocks_broken[0].tile, ForegroundTile::STONE);
}

#[test]
fn spawn_work_and_population_are_bounded() {
    let mut terrain = floor_world(192);
    let system = LifeformSystem::with_built_ins();
    let mut entities = EntityWorld::new();
    let target = target(&mut entities, [96.0, 38.0]);
    let mut config = immediate_config();
    config.player_chunk_radius = [8, 0];
    config.max_player_chunks_per_spawn = 2;
    config.spawn_attempts_per_chunk = 3;
    config.maximum_lifeforms = 1;
    config.maximum_lifeforms_per_chunk = 1;
    let mut simulation = LifeformSimulation::new(config);

    let update = simulation.update(
        &system,
        &mut entities,
        target,
        &mut terrain,
        material(),
        LifeformSpawnView::new([96.0, 38.0], [-20.0, -20.0], [-10.0, -10.0]),
        Duration::from_millis(1),
        PhysicsConfig::default(),
    );

    assert!(update.player_chunks_checked <= 2);
    assert!(update.spawn_attempts <= 6);
    assert!(entities.query::<&Lifeform>().iter().count() <= 1);
}

#[test]
fn glowgnats_are_reserved_for_dense_machinery_attention() {
    let system = LifeformSystem::with_built_ins();
    for hash in 0..1_000 {
        assert_eq!(
            system.select_spawn(
                crate::BiomeId::GLOWING_CRYSTAL,
                GLOWGNAT_MIN_MACHINERY_ATTENTION - 1,
                hash,
            ),
            Some(LifeformId::WALKER)
        );
    }
    assert!((0..1_000).any(|hash| {
        system.select_spawn(
            crate::BiomeId::GLOWING_CRYSTAL,
            GLOWGNAT_MIN_MACHINERY_ATTENTION * 3,
            hash,
        ) == Some(LifeformId::GLOWGNAT)
    }));
    assert!((0..1_000).all(|hash| {
        system.select_spawn(
            crate::BiomeId::NORMAL,
            GLOWGNAT_MIN_MACHINERY_ATTENTION * 3,
            hash,
        ) != Some(LifeformId::GLOWGNAT)
    }));
}

#[test]
fn flying_spawn_positions_do_not_require_a_floor() {
    let terrain = World::empty(64, 64, 99).unwrap();
    let position =
        flying_spawn_position_in_chunk(&terrain, ChunkPos { x: 0, y: 0 }, [0.4, 0.3], 123);
    assert!(position.is_some());
}

#[test]
fn glowgnats_attack_only_dense_machinery_areas() {
    fn run(machine_x_positions: &[u32]) -> LifeformSimulationUpdate {
        let mut terrain = floor_world(128);
        for &x in machine_x_positions {
            let bore = terrain
                .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(x, 37))
                .unwrap();
            assert!(terrain.set_furniture_active(bore, true));
        }
        let mut system = LifeformSystem::new();
        let mut definition = LifeformDefinition::glowgnat(LifeformId::GLOWGNAT, "Test Glowgnat");
        definition.attack_damage = 1;
        system.register(definition).unwrap();
        let mut entities = EntityWorld::new();
        let player = target(&mut entities, [100.0, 38.0]);
        system
            .spawn(
                &mut entities,
                LifeformId::GLOWGNAT,
                material(),
                [65.0, 38.0],
            )
            .unwrap();
        let mut config = immediate_config();
        config.player_spawn_interval = Duration::from_secs(60);
        config.machinery_spawn_interval = Duration::from_secs(60);
        let mut simulation = LifeformSimulation::new(config);
        simulation.update(
            &system,
            &mut entities,
            player,
            &mut terrain,
            material(),
            LifeformSpawnView::new([100.0, 38.0], [90.0, 30.0], [110.0, 48.0]),
            Duration::from_millis(1),
            PhysicsConfig::default(),
        )
    }

    let sparse = run(&[64]);
    let dense = run(&[64, 68, 72, 76, 80]);
    assert_eq!(sparse.machine_attacks, 0);
    assert_eq!(dense.machine_attacks, 1);
}

#[test]
fn sparse_surface_machinery_is_peaceful_but_dense_machinery_is_hostile() {
    fn run(machine_x_positions: &[u32]) -> LifeformSimulationUpdate {
        let mut terrain = floor_world(128);
        for &x in machine_x_positions {
            let bore = terrain
                .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(x, 37))
                .unwrap();
            assert!(terrain.set_furniture_active(bore, true));
        }
        let system = LifeformSystem::with_built_ins();
        let mut entities = EntityWorld::new();
        let player = target(&mut entities, [100.0, 38.0]);
        system
            .spawn(&mut entities, LifeformId::WALKER, material(), [67.0, 38.0])
            .unwrap();
        let mut config = immediate_config();
        config.minimum_hostile_attention = DEFAULT_HOSTILE_MACHINERY_ATTENTION;
        config.player_spawn_interval = Duration::from_secs(60);
        config.machinery_spawn_interval = Duration::from_secs(60);
        LifeformSimulation::new(config).update(
            &system,
            &mut entities,
            player,
            &mut terrain,
            material(),
            LifeformSpawnView::new([100.0, 38.0], [90.0, 30.0], [110.0, 48.0]),
            Duration::from_millis(1),
            PhysicsConfig::default(),
        )
    }

    let sparse = run(&[64]);
    let dense = run(&[64, 68, 72]);
    assert!(sparse.player_machine_attention < DEFAULT_HOSTILE_MACHINERY_ATTENTION);
    assert_eq!(sparse.machine_attacks, 0);
    assert!(dense.player_machine_attention >= DEFAULT_HOSTILE_MACHINERY_ATTENTION);
    assert_eq!(dense.machine_attacks, 1);
}

#[test]
fn nearby_lifeforms_steer_apart() {
    let mut grid = HashMap::new();
    grid.insert((0, 0), vec![[0.0, 0.0], [1.0, 0.0]]);
    assert!(separation_steering([0.0, 0.0], false, &grid)[0] < 0.0);
    assert!(separation_steering([1.0, 0.0], false, &grid)[0] > 0.0);
}
