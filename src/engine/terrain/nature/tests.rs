use super::*;
use crate::{
    ForegroundTile, ItemId, ItemStack, LASER_BORE_MAX_LENGTH, LaserDrillAim, ObjectPlacementError,
};

fn supported_world() -> World {
    let mut world = World::empty(128, 128, 7).unwrap();
    world
        .set_tile(10, 11, Layer::Foreground, ForegroundTile::GRASS)
        .unwrap();
    world
        .set_tile(20, 9, Layer::Foreground, ForegroundTile::GRASS)
        .unwrap();
    world
}

fn power_laser_bore(world: &mut World) -> PowerSystem {
    for x in [9, 11, 12] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(9, 6))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(11, 5))
        .unwrap();
    let mut power = PowerSystem::new();
    power.distribute(world, 0.5, Duration::from_secs(1));
    power
}

#[test]
fn breaking_a_root_removes_its_objects() {
    let mut world = supported_world();
    world
        .place_natural_object(
            NaturalObject::GRASS,
            TilePos::new(10, 10),
            TilePos::new(10, 11),
        )
        .unwrap();
    assert_eq!(world.object_count(), 1);
    world
        .set_tile(10, 11, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert_eq!(world.object_count(), 0);
}

#[test]
fn grass_chooses_a_persistent_random_variant_without_growing() {
    let mut world = World::empty(20, 8, 7).unwrap();
    let mut grass = Vec::new();
    for x in 1..19 {
        world
            .set_tile(x, 4, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        grass.push(
            world
                .place_natural_object(NaturalObject::GRASS, TilePos::new(x, 3), TilePos::new(x, 4))
                .unwrap(),
        );
    }
    let variants = grass
        .iter()
        .map(|&id| world.object(id).unwrap().variant())
        .collect::<Vec<_>>();
    assert!(variants.contains(&0));
    assert!(variants.contains(&1));

    let update = world.update_decorations(Duration::from_secs(120), 64);
    assert_eq!(update.objects_grown, 0);
    assert_eq!(
        grass
            .iter()
            .map(|&id| world.object(id).unwrap().variant())
            .collect::<Vec<_>>(),
        variants
    );
}

#[test]
fn default_natural_growth_rates_are_deliberately_slow() {
    let config = NatureSimulationConfig::default();
    assert_eq!(config.grass_spawn_chance, 6);
    assert_eq!(config.vine_spawn_chance, 4);
    assert_eq!(config.grass_spread_chance, 8);
}

#[test]
fn vine_grows_without_scanning_unrelated_objects() {
    let mut world = supported_world();
    let vine = world
        .place_natural_object(
            NaturalObject::VINE,
            TilePos::new(20, 10),
            TilePos::new(20, 9),
        )
        .unwrap();
    let update = world.update_decorations(Duration::from_secs(8), 8);
    assert_eq!(update.objects_processed, 1);
    assert_eq!(update.objects_grown, 1);
    assert_eq!(world.object(vine).unwrap().size(), [1, 2]);
}

#[test]
fn occupied_cells_prevent_overlapping_objects() {
    let mut world = supported_world();
    world
        .place_natural_object(
            NaturalObject::GRASS,
            TilePos::new(10, 10),
            TilePos::new(10, 11),
        )
        .unwrap();
    assert!(matches!(
        world.place_natural_object(
            NaturalObject::PEBBLE,
            TilePos::new(10, 10),
            TilePos::new(10, 11),
        ),
        Err(ObjectPlacementError::Occupied(_))
    ));
}

#[test]
fn nature_ticks_spawn_valid_plants_and_spread_exposed_grass() {
    let mut world = World::empty(8, 8, 19).unwrap();
    world
        .set_tile(2, 4, Layer::Foreground, ForegroundTile::GRASS)
        .unwrap();
    world
        .set_tile(4, 2, Layer::Foreground, ForegroundTile::GRASS)
        .unwrap();
    world
        .set_tile(5, 4, Layer::Foreground, ForegroundTile::GRASS)
        .unwrap();
    world
        .set_tile(6, 4, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    let config = NatureSimulationConfig {
        horizontal_radius_tiles: 8,
        vertical_radius_tiles: 8,
        columns_per_tick: 8,
        max_columns_per_update: 8,
        object_update_budget: 8,
        grass_spawn_chance: 1,
        vine_spawn_chance: 1,
        grass_spread_chance: 1,
    };

    let early = world.update_nature(Duration::from_millis(999), TilePos::new(4, 4), config);
    assert_eq!(early.columns_scanned, 0);
    let update = world.update_nature(Duration::from_millis(1), TilePos::new(4, 4), config);

    assert!(update.grass_spawned >= 1);
    assert!(update.vines_spawned >= 1);
    assert_eq!(update.grass_tiles_spread, 1);
    assert_eq!(update.changed_tiles(), &[TilePos::new(6, 4)]);
    assert_eq!(
        world.tile(6, 4, Layer::Foreground).unwrap(),
        ForegroundTile::GRASS
    );
    assert!(world.objects().any(|object| {
        object.object_type() == NaturalObject::GRASS && object.anchor() == TilePos::new(2, 3)
    }));
    assert!(world.objects().any(|object| {
        object.object_type() == NaturalObject::VINE && object.anchor() == TilePos::new(4, 3)
    }));
}

#[test]
fn nature_work_is_capped_after_a_long_frame() {
    let mut world = World::empty(128, 64, 3).unwrap();
    let config = NatureSimulationConfig {
        horizontal_radius_tiles: 128,
        vertical_radius_tiles: 64,
        columns_per_tick: 16,
        max_columns_per_update: 3,
        ..NatureSimulationConfig::default()
    };
    let update = world.update_nature(Duration::from_secs(30), TilePos::new(64, 32), config);
    assert_eq!(update.columns_scanned, 3);
}

#[test]
fn laser_bore_destroys_a_two_tile_row_after_three_scheduled_ticks() {
    let mut world = World::empty(16, 80, 0).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(6, 12, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    let power = power_laser_bore(&mut world);

    let beam = world
        .laser_bore_beam(world.object(bore).unwrap(), power.is_powered(bore))
        .unwrap();
    assert_eq!(beam.length_tiles, 4);
    assert_eq!(beam.first_x, 5);
    assert_eq!(beam.width, 2);
    assert_eq!(beam.target_y, Some(12));

    for _ in 0..2 {
        let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        assert_eq!(update.tiles_destroyed, 0);
        assert_eq!(
            world.tile(5, 12, Layer::Foreground).unwrap(),
            ForegroundTile::STONE
        );
        assert_eq!(
            world.tile(6, 12, Layer::Foreground).unwrap(),
            ForegroundTile::STONE
        );
    }
    assert_eq!(
        world
            .block_health(TilePos::new(5, 12), Layer::Foreground)
            .unwrap()
            .unwrap()
            .current(),
        12
    );
    let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    assert_eq!(update.tiles_destroyed, 2);
    assert_eq!(
        update.changed_tiles(),
        &[TilePos::new(5, 12), TilePos::new(6, 12)]
    );
    assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
    assert_eq!(world.tile(6, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
    assert_eq!(
        world.container(bore).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 2)
    );
    assert!(world.object(bore).is_some());
}

#[test]
fn drill_engineer_increases_damage_per_cycle() {
    let target = TilePos::new(4, 4);
    let mut normal = World::empty(12, 12, 0).unwrap();
    let mut improved = World::empty(12, 12, 0).unwrap();
    for world in [&mut normal, &mut improved] {
        world
            .set_tile(target.x, target.y, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }

    assert!(apply_drill_damage(&mut normal, target, true, 100).is_none());
    assert!(apply_drill_damage(&mut improved, target, true, 150).is_none());
    let normal_health = normal
        .block_health(target, Layer::Foreground)
        .unwrap()
        .unwrap()
        .current();
    let improved_health = improved
        .block_health(target, Layer::Foreground)
        .unwrap()
        .unwrap()
        .current();
    assert!(improved_health < normal_health);
}

#[test]
fn drill_engineer_extends_bore_depth() {
    let mut world = World::empty(16, 520, 0).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let target_y = 8 + LASER_BORE_MAX_LENGTH + 10;
    world
        .set_tile(5, target_y, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    assert_eq!(
        world
            .laser_bore_beam(world.object(bore).unwrap(), true)
            .unwrap()
            .target_y,
        None
    );

    world.specialists.push(crate::SpecialistRecord::new(
        crate::SpecialistId::DRILL_ENGINEER,
        crate::ObjectId::from_raw(999),
        [0.0, 0.0],
    ));
    assert_eq!(
        world
            .laser_bore_beam(world.object(bore).unwrap(), true)
            .unwrap()
            .target_y,
        Some(target_y)
    );
}

#[test]
fn red_shaft_bore_destroys_four_tile_rows() {
    let mut world = World::empty(32, 80, 0).unwrap();
    for x in [2, 5, 6, 10, 15, 18, 19] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in 11..=14 {
        world
            .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(2, 6))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(5, 5))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(18, 5))
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::RED_SHAFT_BORE, TilePos::new(10, 5))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert!(power.is_powered(bore));

    let beam = world
        .red_shaft_bore_beam(world.object(bore).unwrap(), true)
        .unwrap();
    assert_eq!(beam.first_x, 11);
    assert_eq!(beam.width, 4);
    assert_eq!(beam.length_tiles, 4);
    assert_eq!(beam.target_y, Some(12));

    for _ in 0..2 {
        assert_eq!(
            world
                .update_decorations_with_power(Duration::from_secs(1), 8, &power)
                .tiles_destroyed,
            0
        );
    }
    let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    assert_eq!(update.tiles_destroyed, 4);
    assert_eq!(
        update.changed_tiles(),
        &[
            TilePos::new(11, 12),
            TilePos::new(12, 12),
            TilePos::new(13, 12),
            TilePos::new(14, 12),
        ]
    );
    for x in 11..=14 {
        assert_eq!(world.tile(x, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
    }
    assert_eq!(
        world.container(bore).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 4)
    );
}

#[test]
fn aimed_laser_drill_mines_the_first_tile_on_its_selected_ray() {
    let mut world = World::empty(80, 64, 0).unwrap();
    for x in [20, 22, 23, 30, 32] {
        world
            .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let target = TilePos::new(35, 11);
    world
        .set_tile(target.x, target.y, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(20, 5))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(22, 4))
        .unwrap();
    let drill = world
        .place_furniture(FurnitureObject::LASER_DRILL, TilePos::new(30, 5))
        .unwrap();
    assert_eq!(world.laser_drill_aim(drill), Some(LaserDrillAim::Down));
    assert!(world.set_laser_drill_aim(drill, LaserDrillAim::Right));
    assert!(world.set_furniture_active(drill, true));
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert!(power.is_powered(drill));

    let beam = world
        .laser_drill_beam(world.object(drill).unwrap(), true)
        .unwrap();
    assert_eq!(beam.aim, LaserDrillAim::Right);
    assert_eq!(beam.target, Some(target));
    for _ in 0..2 {
        assert_eq!(
            world
                .update_decorations_with_power(Duration::from_secs(1), 8, &power)
                .tiles_destroyed,
            0
        );
    }
    let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    assert_eq!(update.changed_tiles(), &[target]);
    assert_eq!(
        world.tile(target.x, target.y, Layer::Foreground).unwrap(),
        TileId::EMPTY
    );
    assert_eq!(
        world.container(drill).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );
}

#[test]
fn laser_bore_stays_idle_until_activated_and_stops_when_deactivated() {
    let mut world = World::empty(16, 80, 0).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    let power = power_laser_bore(&mut world);

    assert!(!world.object(bore).unwrap().is_active());
    assert!(
        world
            .laser_bore_beam(world.object(bore).unwrap(), power.is_powered(bore))
            .is_none()
    );
    let idle = world.update_decorations(Duration::from_secs(10), 8);
    assert_eq!(idle.objects_processed, 0);

    assert!(world.set_furniture_active(bore, true));
    world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    assert!(world.set_furniture_active(bore, false));
    let stopped = world.update_decorations(Duration::from_secs(10), 8);
    assert_eq!(stopped.tiles_destroyed, 0);
    assert_eq!(
        world.tile(5, 12, Layer::Foreground).unwrap(),
        ForegroundTile::STONE
    );
}

#[test]
fn active_laser_bore_pauses_until_its_grid_is_energized() {
    let mut world = World::empty(16, 80, 0).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    let mut power = PowerSystem::new();
    power.update(&world);

    for _ in 0..4 {
        world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    }
    assert_eq!(
        world.tile(5, 12, Layer::Foreground).unwrap(),
        ForegroundTile::STONE
    );
    assert_eq!(world.object(bore).unwrap().growth_stage(), 0);

    power = power_laser_bore(&mut world);
    assert!(power.is_powered(bore));
    for _ in 0..3 {
        world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    }
    assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
}

#[test]
fn laser_bore_pauses_without_destroying_a_block_when_storage_is_full() {
    let mut world = World::empty(16, 80, 0).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    let power = power_laser_bore(&mut world);
    for slot in 0..usize::from(crate::LASER_BORE_SLOTS) {
        assert!(
            world
                .container_mut(bore)
                .unwrap()
                .set_slot(slot, ItemStack::new(ItemId::DIRT_BLOCK, 999),)
        );
    }

    for _ in 0..4 {
        world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    }
    assert_eq!(
        world.tile(5, 12, Layer::Foreground).unwrap(),
        ForegroundTile::STONE
    );

    assert!(world.container_mut(bore).unwrap().set_slot(0, None));
    let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    assert_eq!(update.tiles_destroyed, 1);
    assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
    assert_eq!(
        world.container(bore).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );
}

#[test]
fn laser_bore_mines_outside_the_players_active_area() {
    let mut world = World::empty(512, 80, 0).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    let power = power_laser_bore(&mut world);
    let config = NatureSimulationConfig {
        horizontal_radius_tiles: 0,
        vertical_radius_tiles: 0,
        columns_per_tick: 1,
        max_columns_per_update: 1,
        object_update_budget: 1,
        ..NatureSimulationConfig::default()
    };
    let player = TilePos::new(500, 70);

    for _ in 0..2 {
        let update = world.update_nature_with_power(Duration::from_secs(1), player, config, &power);
        assert_eq!(update.decorations.objects_processed, 1);
        assert_eq!(update.decorations.tiles_destroyed, 0);
        assert_eq!(update.columns_scanned, 1);
    }
    let update = world.update_nature_with_power(Duration::from_secs(1), player, config, &power);
    assert_eq!(update.decorations.objects_processed, 1);
    assert_eq!(update.decorations.tiles_destroyed, 1);
    assert_eq!(update.changed_tiles(), &[TilePos::new(5, 12)]);
    assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
}

#[test]
fn laser_bore_never_scans_beyond_four_hundred_tiles() {
    let mut world = World::empty(8, 500, 0).unwrap();
    for x in [2, 5] {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(3, 406, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 3))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));

    let beam = world
        .laser_bore_beam(world.object(bore).unwrap(), true)
        .unwrap();
    assert_eq!(beam.first_y, 6);
    assert_eq!(beam.length_tiles, 400);
    assert_eq!(beam.target_y, None);
    for _ in 0..4 {
        world.update_decorations(Duration::from_secs(1), 8);
    }
    assert_eq!(
        world.tile(3, 406, Layer::Foreground).unwrap(),
        ForegroundTile::STONE
    );
}

#[test]
fn laser_bore_tracks_and_mines_targets_beyond_u8_range() {
    let mut world = World::empty(16, 450, 0).unwrap();
    for x in [2, 5, 6, 8, 9] {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(3, 306, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 3))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 4))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(8, 3))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));

    for _ in 0..3 {
        world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    }
    assert_eq!(
        world.tile(3, 306, Layer::Foreground).unwrap(),
        TileId::EMPTY
    );
}

#[test]
fn dirt_becoming_grass_keeps_generic_surface_decorations() {
    let mut world = World::empty(8, 8, 0).unwrap();
    world
        .set_tile(2, 3, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    let pebble = world
        .place_natural_object(
            NaturalObject::PEBBLE,
            TilePos::new(2, 2),
            TilePos::new(2, 3),
        )
        .unwrap();
    world
        .set_tile(2, 3, Layer::Foreground, ForegroundTile::GRASS)
        .unwrap();
    assert!(world.object(pebble).is_some());
}

#[test]
fn decoration_revision_tracks_placement_and_growth() {
    let mut world = supported_world();
    let initial = world.object_revision();
    world
        .place_natural_object(
            NaturalObject::VINE,
            TilePos::new(20, 10),
            TilePos::new(20, 9),
        )
        .unwrap();
    let placed = world.object_revision();
    assert_ne!(placed, initial);
    world.update_decorations(Duration::from_secs(8), 8);
    assert_ne!(world.object_revision(), placed);
}
