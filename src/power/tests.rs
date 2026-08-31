use super::*;
use crate::{ForegroundTile, FurnitureObject};

fn support(world: &mut World, x: u32, y: u32, width: u32) {
    for support_x in x..x + width {
        world
            .set_tile(support_x, y, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
}

fn basic_grid() -> (World, ObjectId, ObjectId, ObjectId) {
    let mut world = World::empty(64, 32, 0).unwrap();
    support(&mut world, 2, 12, 2);
    support(&mut world, 8, 12, 1);
    support(&mut world, 14, 12, 3);
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
        .unwrap();
    let pylon = world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(8, 10))
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(14, 9))
        .unwrap();
    (world, solar, pylon, bore)
}

#[test]
fn pylon_energizes_a_visible_machine_from_a_visible_generator() {
    let (mut world, solar, pylon, bore) = basic_grid();
    let mut power = PowerSystem::new();
    let update = power.update(&world);
    let flow = power.distribute(&mut world, 0.5, Duration::from_secs(1));

    assert!(update.topology_rebuilt);
    assert_eq!(update.node_count, 3);
    assert_eq!(update.connection_count, 2);
    assert_eq!(update.network_count, 1);
    assert_eq!(flow.generated_milli, 12_000);
    assert!(power.is_powered(solar));
    assert!(power.is_powered(pylon));
    assert!(power.is_powered(bore));
}

#[test]
fn generators_and_consumers_require_a_pylon_endpoint() {
    let mut world = World::empty(32, 20, 0).unwrap();
    support(&mut world, 2, 12, 2);
    support(&mut world, 8, 12, 3);
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(8, 9))
        .unwrap();

    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(power.candidate_connection_count(), 0);
    assert_eq!(power.connection_count(), 0);
    assert!(!power.is_powered(bore));
}

#[test]
fn range_is_inclusive_and_uses_socket_distance() {
    let mut world = World::empty(64, 32, 0).unwrap();
    support(&mut world, 2, 12, 1);
    support(&mut world, 24, 12, 1);
    support(&mut world, 47, 12, 1);
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(2, 10))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(24, 10))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(47, 10))
        .unwrap();

    let mut power = PowerSystem::new();
    power.update(&world);
    assert_eq!(power.candidate_connection_count(), 1);
    assert_eq!(power.connection_count(), 1);
}

#[test]
fn foreground_los_changes_recheck_only_crossed_candidates() {
    let (mut world, _, _, bore) = basic_grid();
    support(&mut world, 50, 20, 1);
    let mut power = PowerSystem::new();
    power.update(&world);

    world
        .set_tile(11, 10, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let blocked = power.update(&world);
    assert_eq!(blocked.edges_rechecked, 1);
    assert!(blocked.connections_changed);
    assert!(!power.is_powered(bore));
    assert_eq!(power.connection_count(), 1);

    world
        .set_tile(50, 20, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    let unrelated = power.update(&world);
    assert_eq!(unrelated.edges_rechecked, 0);
    assert!(!unrelated.connections_changed);

    world
        .set_tile(11, 10, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    let restored = power.update(&world);
    assert_eq!(restored.edges_rechecked, 1);
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert!(power.is_powered(bore));
}

#[test]
fn diagonal_corner_blockers_prevent_connections() {
    let mut world = World::empty(32, 32, 0).unwrap();
    support(&mut world, 2, 16, 1);
    support(&mut world, 10, 8, 1);
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(2, 14))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(10, 6))
        .unwrap();
    world
        .set_tile(6, 9, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();

    let mut power = PowerSystem::new();
    power.update(&world);
    assert_eq!(power.candidate_connection_count(), 1);
    assert_eq!(power.connection_count(), 0);
}

#[test]
fn power_propagates_across_multiple_pylons() {
    let mut world = World::empty(80, 32, 0).unwrap();
    support(&mut world, 2, 12, 2);
    support(&mut world, 14, 12, 1);
    support(&mut world, 28, 12, 1);
    support(&mut world, 40, 12, 3);
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(14, 10))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(28, 10))
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(40, 9))
        .unwrap();

    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(power.connection_count(), 3);
    assert!(power.is_powered(bore));
}

#[test]
fn pylon_connections_are_limited_to_ten_nearest_nodes() {
    let mut world = World::empty(72, 24, 0).unwrap();
    support(&mut world, 30, 12, 1);
    let pylon = world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(30, 10))
        .unwrap();
    for x in [8, 11, 14, 17, 20, 23, 34, 37, 40, 43, 46] {
        support(&mut world, x, 12, 2);
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(x, 9))
            .unwrap();
    }

    let mut power = PowerSystem::new();
    power.update(&world);
    let pylon_connections = power
        .connections()
        .iter()
        .filter(|connection| connection.endpoints().contains(&pylon))
        .count();

    assert_eq!(power.candidate_connection_count(), 11);
    assert_eq!(pylon_connections, PYLON_CONNECTION_LIMIT);
}

#[test]
fn power_connector_has_five_connections() {
    let mut world = World::empty(48, 24, 0).unwrap();
    world
        .set_tile(20, 11, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(20, 10))
        .unwrap();
    for x in [12, 14, 16, 21, 23, 25] {
        support(&mut world, x, 12, 2);
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(x, 9))
            .unwrap();
    }
    let mut power = PowerSystem::new();
    power.update(&world);
    let connections: Vec<_> = power
        .connections()
        .iter()
        .filter(|connection| connection.endpoints().contains(&connector))
        .collect();

    assert_eq!(connections.len(), POWER_CONNECTOR_CONNECTION_LIMIT);
}

#[test]
fn power_connector_range_is_eight_tiles() {
    let mut world = World::empty(32, 24, 0).unwrap();
    world
        .set_tile(10, 11, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(10, 10))
        .unwrap();
    support(&mut world, 17, 12, 2);
    let nearby = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(17, 9))
        .unwrap();
    support(&mut world, 19, 12, 2);
    let distant = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(19, 9))
        .unwrap();

    let mut power = PowerSystem::new();
    power.update(&world);

    assert_eq!(power.candidate_connection_count(), 1);
    assert_eq!(power.connections().len(), 1);
    assert!(power.connections()[0].endpoints().contains(&connector));
    assert!(power.connections()[0].endpoints().contains(&nearby));
    assert!(!power.connections()[0].endpoints().contains(&distant));
}

#[test]
fn power_connector_cables_pass_through_foreground_blocks() {
    let mut world = World::empty(32, 24, 0).unwrap();
    world
        .set_tile(8, 10, Layer::Background, crate::BackgroundTile::STONE_WALL)
        .unwrap();
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(8, 10))
        .unwrap();
    support(&mut world, 14, 12, 2);
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(14, 9))
        .unwrap();
    world
        .set_tile(11, 10, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();

    let mut power = PowerSystem::new();
    power.update(&world);

    assert_eq!(power.connection_count(), 1);
    assert_eq!(power.connections()[0].endpoints(), [connector, solar]);

    world
        .set_tile(11, 10, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    let update = power.update(&world);
    assert_eq!(update.edges_rechecked, 0);
    assert!(!update.connections_changed);
}

#[test]
fn pylon_to_connector_links_use_the_pylon_range() {
    let mut world = World::empty(48, 24, 0).unwrap();
    world
        .set_tile(10, 10, Layer::Background, crate::BackgroundTile::STONE_WALL)
        .unwrap();
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(10, 10))
        .unwrap();
    support(&mut world, 28, 12, 1);
    let pylon = world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(28, 10))
        .unwrap();

    let mut power = PowerSystem::new();
    power.update(&world);

    assert_eq!(power.candidate_connection_count(), 1);
    assert_eq!(power.connections().len(), 1);
    assert_eq!(power.connections()[0].endpoints(), [connector, pylon]);
}

#[test]
fn pylon_connects_only_to_a_machine_without_an_existing_connection() {
    let mut world = World::empty(48, 24, 0).unwrap();
    support(&mut world, 20, 12, 2);
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(20, 9))
        .unwrap();
    for x in [7, 12, 28] {
        support(&mut world, x, 12, 1);
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(x, 10))
            .unwrap();
    }

    let mut power = PowerSystem::new();
    power.update(&world);
    let machine_connections = power
        .connections()
        .iter()
        .filter(|connection| connection.endpoints().contains(&solar))
        .count();

    assert_eq!(machine_connections, MACHINE_CONNECTION_LIMIT);
}

#[test]
fn unrelated_furniture_does_not_rebuild_power_topology() {
    let (mut world, _, _, _) = basic_grid();
    let mut power = PowerSystem::new();
    assert!(power.update(&world).topology_rebuilt);
    support(&mut world, 24, 12, 2);
    world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(24, 10))
        .unwrap();
    assert!(!power.update(&world).topology_rebuilt);
}

#[test]
fn localized_node_changes_update_candidates_without_a_full_rebuild() {
    let (mut world, _, _, _) = basic_grid();
    let mut power = PowerSystem::new();
    let initial = power.update(&world);
    assert!(initial.full_topology_rebuild);
    assert_eq!(power.node_count(), 3);
    assert_eq!(power.candidate_connection_count(), 2);

    support(&mut world, 10, 12, 2);
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(10, 9))
        .unwrap();
    let inserted = power.update(&world);
    assert!(inserted.topology_rebuilt);
    assert!(!inserted.full_topology_rebuild);
    assert_eq!(inserted.topology_changes_applied, 1);
    assert_eq!(power.node_count(), 4);
    assert_eq!(power.candidate_connection_count(), 3);
    assert!(
        power
            .connections()
            .iter()
            .any(|connection| connection.endpoints().contains(&solar))
    );

    assert!(world.remove_object(solar).is_some());
    let removed = power.update(&world);
    assert!(removed.topology_rebuilt);
    assert!(!removed.full_topology_rebuild);
    assert_eq!(removed.topology_changes_applied, 1);
    assert_eq!(power.node_count(), 3);
    assert_eq!(power.candidate_connection_count(), 2);
    assert!(
        power
            .connections()
            .iter()
            .all(|connection| !connection.endpoints().contains(&solar))
    );
}

#[test]
fn powered_cable_shape_changes_use_the_full_rebuild_fallback() {
    let mut world = World::empty(24, 32, 0).unwrap();
    world
        .set_tile(12, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(12, 3))
        .unwrap();
    world
        .place_or_extend_powered_cable(TilePos::new(12, 3))
        .unwrap();
    let mut power = PowerSystem::new();
    power.update(&world);

    world
        .place_or_extend_powered_cable(TilePos::new(12, 3))
        .unwrap();
    let update = power.update(&world);
    assert!(update.topology_rebuilt);
    assert!(update.full_topology_rebuild);
    assert_eq!(update.topology_changes_applied, 0);
}

#[test]
fn socket_metadata_is_present_for_every_power_definition() {
    for definition in BUILT_IN_FURNITURE
        .iter()
        .copied()
        .filter(|definition| definition.power_role().is_some())
    {
        assert!(definition.power_socket_half_tiles().is_some());
        assert_eq!(
            crate::furniture_definition(definition.object_type()),
            Some(definition)
        );
    }
}

#[test]
fn cable_chunk_index_returns_each_connection_once_per_chunk() {
    let (world, _, _, _) = basic_grid();
    let mut power = PowerSystem::new();
    power.update(&world);
    assert_eq!(
        power.connections_in_chunk(ChunkPos { x: 0, y: 0 }).count(),
        2
    );
    assert_eq!(crate::CHUNK_SIZE, 64);
}

#[test]
fn solar_generation_stops_at_night() {
    let (mut world, solar, pylon, bore) = basic_grid();
    let mut power = PowerSystem::new();
    let flow = power.distribute(&mut world, 0.9, Duration::from_secs(1));

    assert!(!flow.daytime);
    assert_eq!(flow.generated_milli, 0);
    assert!(!power.is_powered(solar));
    assert!(!power.is_powered(pylon));
    assert!(!power.is_powered(bore));
}

#[test]
fn procurement_terminal_draws_its_small_declared_power_load() {
    let mut world = World::empty(40, 32, 0).unwrap();
    support(&mut world, 2, 12, 2);
    support(&mut world, 8, 12, 1);
    support(&mut world, 14, 12, 2);
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(8, 10))
        .unwrap();
    let terminal = world
        .place_furniture(FurnitureObject::PROCUREMENT_TERMINAL, TilePos::new(14, 10))
        .unwrap();
    let mut power = PowerSystem::new();

    let flow = power.distribute(&mut world, 0.5, Duration::from_secs(1));

    assert!(power.is_powered(solar));
    assert!(power.is_powered(terminal));
    assert_eq!(
        flow.consumed_milli,
        crate::terrain::PROCUREMENT_TERMINAL_DEMAND_MILLI_PER_SECOND as u64
    );
}

#[test]
fn roofed_solar_array_does_not_generate_power() {
    let (mut world, solar, pylon, bore) = basic_grid();
    world
        .set_tile(2, 4, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let mut power = PowerSystem::new();

    let flow = power.distribute(&mut world, 0.5, Duration::from_secs(1));

    assert!(!world.solar_array_has_sky_access(solar));
    assert_eq!(flow.generated_milli, 0);
    assert!(!power.is_powered(solar));
    assert!(!power.is_powered(pylon));
    assert!(!power.is_powered(bore));
}

#[test]
fn solar_array_below_minus_one_hundred_metres_does_not_generate_power() {
    let mut world = World::empty(24, 300, 0).unwrap();
    support(&mut world, 2, 190, 2);
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 187))
        .unwrap();
    let mut power = PowerSystem::new();

    let flow = power.distribute(&mut world, 0.5, Duration::from_secs(1));

    assert!(world.elevation_decimetres(189.0) < -1_000);
    assert!(!world.solar_array_has_sky_access(solar));
    assert_eq!(flow.generated_milli, 0);
    assert!(!power.is_powered(solar));
}

#[test]
fn battery_charges_by_day_and_supplies_an_active_bore_at_night() {
    let (mut world, _, _, bore) = basic_grid();
    support(&mut world, 10, 12, 2);
    let battery = world
        .place_furniture(FurnitureObject::BATTERY, TilePos::new(10, 10))
        .unwrap();
    let mut power = PowerSystem::new();

    let daylight = power.distribute(&mut world, 0.5, Duration::from_secs(10));
    assert_eq!(daylight.generated_milli, 120_000);
    assert_eq!(world.battery_charge_milli(battery), Some(120_000));

    assert!(world.set_furniture_active(bore, true));
    let night = power.distribute(&mut world, 0.9, Duration::from_secs(1));
    assert!(power.is_powered(bore));
    assert_eq!(night.consumed_milli, 8_000);
    assert_eq!(world.battery_charge_milli(battery), Some(112_000));

    power.distribute(&mut world, 0.9, Duration::from_secs(14));
    assert_eq!(world.battery_charge_milli(battery), Some(0));
    power.distribute(&mut world, 0.9, Duration::from_secs(1));
    assert!(!power.is_powered(bore));
}

#[test]
fn insufficient_generation_load_sheds_consumers_in_stable_object_order() {
    let (mut world, _, _, bore) = basic_grid();
    support(&mut world, 18, 12, 2);
    let turret = world
        .place_furniture(FurnitureObject::TURRET, TilePos::new(18, 10))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    assert!(world.set_furniture_active(turret, true));
    let mut power = PowerSystem::new();

    let flow = power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(flow.generated_milli, 12_000);
    assert_eq!(flow.consumed_milli, 8_000);
    assert!(power.is_powered(bore));
    assert!(!power.is_powered(turret));
}

#[test]
fn cable_endpoints_connect_relays_and_power_lift_motion_without_anchors() {
    let mut world = World::empty(40, 36, 0).unwrap();
    world
        .set_tile(12, 3, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    for _ in 0..20 {
        world
            .place_or_extend_powered_cable(TilePos::new(12, 4))
            .unwrap();
    }
    let cable = world.object_at(TilePos::new(12, 8)).unwrap().id();
    world
        .set_tile(14, 4, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(13, 4))
        .unwrap();
    let lift = world.place_cargo_lift(TilePos::new(12, 4)).unwrap();

    support(&mut world, 3, 24, 2);
    support(&mut world, 6, 24, 1);
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(3, 21))
        .unwrap();
    let pylon = world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 22))
        .unwrap();
    let mut power = PowerSystem::new();

    let update = power.update(&world);
    let idle = power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(update.connection_count, 4);
    assert_eq!(idle.consumed_milli, 0);
    assert!(power.is_powered(solar));
    assert!(power.is_powered(pylon));
    assert!(power.is_powered(connector));
    assert!(power.is_powered(cable));
    assert!(power.is_powered(lift));
    let top_connection = power.connections().iter().find(|connection| {
        let endpoints = connection.endpoints();
        endpoints.contains(&cable) && endpoints.contains(&connector)
    });
    assert!(top_connection.is_some_and(|connection| {
        connection.start() == [12.0, 4.0] || connection.end() == [12.0, 4.0]
    }));
    let bottom_connection = power.connections().iter().find(|connection| {
        let endpoints = connection.endpoints();
        endpoints.contains(&cable) && endpoints.contains(&pylon)
    });
    assert!(bottom_connection.is_some_and(|connection| {
        connection.start() == [12.0, 23.0] || connection.end() == [12.0, 23.0]
    }));

    assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
    let moving = power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(moving.consumed_milli, 10_000);
    let registry = crate::ItemRegistry::with_built_ins();
    assert_eq!(
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
        1
    );
    assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 10.0);

    assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Up));
    let unpowered = power.distribute(&mut world, 0.9, Duration::from_secs(1));
    assert_eq!(unpowered.consumed_milli, 0);
    assert!(!power.is_powered(lift));
    assert_eq!(
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
        0
    );
    assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 10.0);
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Up)
    );

    let powered = power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(powered.consumed_milli, 10_000);
    assert_eq!(
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
        1
    );
    assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 4.0);
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Idle)
    );

    assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
    for _ in 0..3 {
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
    }
    assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 22.0);
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Idle)
    );
}

#[test]
fn lift_station_loads_unloads_and_dispatches_at_the_nearest_stop() {
    let mut world = World::empty(40, 36, 0).unwrap();
    world
        .set_tile(12, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(12, 3))
        .unwrap();
    for _ in 0..20 {
        world
            .place_or_extend_powered_cable(TilePos::new(12, 3))
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(12, 24))
        .unwrap();
    let lift = world.place_cargo_lift(TilePos::new(12, 4)).unwrap();
    support(&mut world, 10, 12, 2);
    let station = world.place_lift_station(TilePos::new(10, 10)).unwrap();
    assert_eq!(
        world.object(station).unwrap().anchor(),
        TilePos::new(10, 10)
    );

    support(&mut world, 3, 24, 2);
    support(&mut world, 6, 24, 1);
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(3, 21))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 22))
        .unwrap();
    assert!(world.set_lift_station_departure(station, CargoLiftDirection::Up));

    let registry = crate::ItemRegistry::with_built_ins();
    let mut power = PowerSystem::new();
    assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
        1
    );
    assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 10.0);
    assert!(world.container(lift).unwrap().is_empty());
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Idle)
    );

    // An empty lift waits for cargo instead of departing from an empty load station.
    world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Idle)
    );
    assert!(
        world
            .container_mut(station)
            .unwrap()
            .try_add(crate::ItemId::STONE_BLOCK, 37, 999)
    );

    let loading = power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(loading.consumed_milli, 0);
    world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
    assert_eq!(
        world.container(station).unwrap().slot(0),
        crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 17)
    );
    assert_eq!(
        world.container(lift).unwrap().slot(0),
        crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 20)
    );
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Idle)
    );

    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
    assert!(world.container(station).unwrap().is_empty());
    assert_eq!(
        world.container(lift).unwrap().slot(0),
        crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 37)
    );
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Up)
    );

    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
    assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 4.0);
    assert!(world.set_lift_station_mode(station, crate::LiftStationMode::Unload));
    assert!(world.set_lift_station_departure(station, CargoLiftDirection::Down));
    assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);

    assert_eq!(world.container(station).unwrap().slot(0), None);
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Idle)
    );

    let unloading = power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert_eq!(unloading.consumed_milli, 0);
    world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
    assert_eq!(
        world.container(station).unwrap().slot(0),
        crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 20)
    );
    assert_eq!(
        world.container(lift).unwrap().slot(0),
        crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 17)
    );

    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
    assert!(world.container(lift).unwrap().is_empty());
    assert_eq!(
        world.container(station).unwrap().slot(0),
        crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 37)
    );
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Down)
    );
}
