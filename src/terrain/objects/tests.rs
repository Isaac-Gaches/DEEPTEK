use super::*;
use crate::{
    CargoLiftDirection, ForegroundTile, FurnitureConfiguration, FurnitureInteraction,
    FurnitureObject, ItemId, ItemStack, ItemTransportRole, Layer, LiftStationConfiguration,
    POWERED_CABLE_OBJECT, ROPE_OBJECT, TargetPriority, TileId, World,
};

fn chest_floor() -> World {
    let mut world = World::empty(8, 8, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 5, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
    }
    world
}

#[test]
fn chest_occupies_a_two_by_two_footprint_and_exposes_interaction() {
    let mut world = chest_floor();
    let id = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();

    assert_eq!(world.object(id).unwrap().size(), [2, 2]);
    for y in 3..=4 {
        for x in 2..=3 {
            assert_eq!(world.object_at(TilePos::new(x, y)).unwrap().id(), id);
        }
    }
    assert_eq!(
        world.furniture_interaction_at(TilePos::new(3, 4)),
        Some((id, FurnitureInteraction::container(40)))
    );
    assert_eq!(world.container(id).unwrap().slots().len(), 40);
    assert!(matches!(
        world.can_place_furniture(FurnitureObject::CHEST, TilePos::new(3, 3)),
        Err(ObjectPlacementError::Occupied(_))
    ));
    world
        .set_tile(2, 3, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    assert_eq!(world.object_at(TilePos::new(2, 3)).unwrap().id(), id);
    assert_eq!(
        world.tile_in_bounds(2, 3, Layer::Foreground),
        ForegroundTile::DIRT
    );
}

#[test]
fn chest_requires_its_whole_floor_and_breaking_either_support_removes_it() {
    let mut world = chest_floor();
    world
        .set_tile(3, 5, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert_eq!(
        world.can_place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3)),
        Err(ObjectPlacementError::RootIsEmpty(TilePos::new(3, 5)))
    );

    world
        .set_tile(3, 5, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();
    world
        .set_tile(3, 5, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert_eq!(world.object_count(), 0);
}

#[test]
fn non_empty_chest_cannot_be_mined_or_unsupported() {
    let mut world = chest_floor();
    let id = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();
    assert!(
        world
            .container_mut(id)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 12),)
    );

    assert!(world.remove_object_at(TilePos::new(3, 4)).is_none());
    assert!(matches!(
        world.set_tile(2, 5, Layer::Foreground, TileId::EMPTY),
        Err(crate::WorldError::ContainerNotEmpty { object }) if object == id
    ));
    assert_eq!(world.object_at(TilePos::new(2, 3)).unwrap().id(), id);
}

#[test]
fn laser_bore_occupies_three_by_three_cells_and_only_requires_edge_supports() {
    let mut world = World::empty(12, 16, 0).unwrap();
    for x in [2, 4] {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }

    let id = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 3))
        .unwrap();
    assert_eq!(world.object(id).unwrap().size(), [3, 3]);
    assert_eq!(
        world.furniture_interaction_at(TilePos::new(3, 4)),
        Some((
            id,
            FurnitureInteraction::machine_with_transport(
                crate::LASER_BORE_SLOTS,
                ItemTransportRole::Output,
            )
            .with_drill_depth_status(),
        ))
    );
    assert!(!world.object(id).unwrap().is_active());
    assert_eq!(
        world.container(id).unwrap().slots().len(),
        usize::from(crate::LASER_BORE_SLOTS)
    );
    for y in 3..=5 {
        for x in 2..=4 {
            assert_eq!(world.object_at(TilePos::new(x, y)).unwrap().id(), id);
        }
    }

    world
        .set_tile(3, 6, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    assert!(world.object(id).is_some());
    world
        .set_tile(2, 6, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert!(world.object(id).is_none());
}

#[test]
fn turret_starts_off_and_exposes_typed_targeting_without_a_container() {
    let mut world = chest_floor();
    let id = world
        .place_furniture(FurnitureObject::TURRET, TilePos::new(2, 3))
        .unwrap();
    let interaction =
        FurnitureInteraction::controlled_machine(FurnitureConfiguration::TargetPriority)
            .with_kill_count_status();

    assert_eq!(
        world.furniture_interaction_at(TilePos::new(3, 4)),
        Some((id, interaction))
    );
    assert!(!world.object(id).unwrap().is_active());
    assert!(world.container(id).is_none());
    assert_eq!(
        world.furniture_target_priority(id),
        Some(TargetPriority::Closest)
    );
    assert!(world.set_furniture_target_priority(id, TargetPriority::Strongest));
    assert_eq!(
        world.furniture_target_priority(id),
        Some(TargetPriority::Strongest)
    );
    assert!(world.set_furniture_active(id, true));
    assert!(world.object(id).unwrap().is_active());
    assert_eq!(world.object(id).unwrap().next_update_tick, u64::MAX);
    assert_eq!(world.turret_kill_count(id), Some(0));
    assert!(world.increment_turret_kill_count(id));
    assert_eq!(world.turret_kill_count(id), Some(1));
}

#[test]
fn rope_extends_from_any_segment_until_the_next_cell_is_blocked() {
    let mut world = World::empty(8, 10, 0).unwrap();
    let support = TilePos::new(3, 1);
    world
        .set_tile(
            support.x,
            support.y,
            Layer::Foreground,
            ForegroundTile::STONE,
        )
        .unwrap();

    let first = world.place_or_extend_rope(TilePos::new(3, 2)).unwrap();
    assert_eq!(first, TilePos::new(3, 2));
    let rope = world.object_at(first).unwrap().id();
    assert_eq!(world.object(rope).unwrap().object_type(), ROPE_OBJECT);

    assert_eq!(
        world.place_or_extend_rope(first).unwrap(),
        TilePos::new(3, 3)
    );
    assert_eq!(
        world.place_or_extend_rope(TilePos::new(3, 2)).unwrap(),
        TilePos::new(3, 4)
    );
    assert_eq!(world.object(rope).unwrap().size(), [1, 3]);
    assert_eq!(world.object_at(TilePos::new(3, 4)).unwrap().id(), rope);

    world
        .set_tile(3, 5, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    assert_eq!(
        world.rope_placement_target(first),
        Err(ObjectPlacementError::Occupied(TilePos::new(3, 5)))
    );

    world
        .set_tile(support.x, support.y, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert!(world.object(rope).is_none());
}

#[test]
fn foreground_tiles_keep_rope_but_are_blocked_by_powered_cable() {
    let mut world = World::empty(12, 16, 0).unwrap();
    world
        .set_tile(3, 1, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let rope_position = world.place_or_extend_rope(TilePos::new(3, 2)).unwrap();
    let rope = world.object_at(rope_position).unwrap().id();

    world
        .set_tile(3, 2, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    assert_eq!(world.object_at(rope_position).unwrap().id(), rope);

    world
        .set_tile(8, 1, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(8, 2))
        .unwrap();
    let cable_position = world
        .place_or_extend_powered_cable(TilePos::new(8, 2))
        .unwrap();
    let cable = world.object_at(cable_position).unwrap().id();

    assert!(matches!(
        world.set_tile(8, 3, Layer::Foreground, ForegroundTile::DIRT),
        Err(crate::WorldError::OccupiedByObject { object }) if object == cable
    ));
    assert_eq!(world.object_at(cable_position).unwrap().id(), cable);
}

#[test]
fn background_tiles_support_blocks_rope_anchors_and_connectors() {
    let mut world = World::empty(16, 20, 0).unwrap();
    for position in [
        TilePos::new(2, 4),
        TilePos::new(5, 4),
        TilePos::new(8, 4),
        TilePos::new(11, 4),
    ] {
        world
            .set_tile(
                position.x,
                position.y,
                Layer::Background,
                crate::BackgroundTile::STONE_WALL,
            )
            .unwrap();
    }

    assert!(world.can_place_tile_adjacent(TilePos::new(2, 4), Layer::Foreground));
    world
        .set_tile(2, 4, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    assert_eq!(
        world.place_or_extend_rope(TilePos::new(5, 4)),
        Ok(TilePos::new(5, 4))
    );
    let anchor = world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(8, 4))
        .unwrap();
    assert_eq!(
        world.place_or_extend_powered_cable(TilePos::new(8, 4)),
        Ok(TilePos::new(8, 5))
    );
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(11, 4))
        .unwrap();
    assert_eq!(world.object(connector).unwrap().root(), TilePos::new(11, 4));

    world
        .set_tile(11, 4, Layer::Background, TileId::EMPTY)
        .unwrap();
    assert!(world.object(connector).is_none());
    assert!(world.object(anchor).is_some());
}

#[test]
fn rope_column_can_start_beside_a_solid_tile() {
    let mut world = World::empty(8, 10, 0).unwrap();
    let support = TilePos::new(3, 3);
    world
        .set_tile(
            support.x,
            support.y,
            Layer::Foreground,
            ForegroundTile::STONE,
        )
        .unwrap();

    let first = world.place_or_extend_rope(TilePos::new(4, 3)).unwrap();
    let rope = world.object_at(first).unwrap().id();
    assert_eq!(world.object(rope).unwrap().root(), support);
    assert_eq!(
        world.place_or_extend_rope(first).unwrap(),
        TilePos::new(4, 4)
    );

    world
        .set_tile(support.x, support.y, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert!(world.object(rope).is_none());
}

#[test]
fn powered_cable_uses_endpoint_anchors_and_hosts_one_persistent_lift() {
    let mut world = World::empty(20, 30, 0).unwrap();
    world
        .set_tile(8, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let top_anchor = world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(8, 3))
        .unwrap();

    for _ in 0..8 {
        world
            .place_or_extend_powered_cable(TilePos::new(8, 3))
            .unwrap();
    }
    let cable = world.object_at(TilePos::new(8, 6)).unwrap().id();
    assert_eq!(
        world.object(cable).unwrap().object_type(),
        POWERED_CABLE_OBJECT
    );
    assert_eq!(world.object(cable).unwrap().size(), [1, 8]);
    assert_eq!(
        world.powered_cable_anchor_placement_target(TilePos::new(8, 6)),
        Ok(TilePos::new(8, 12))
    );
    let bottom_anchor = world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(8, 12))
        .unwrap();
    assert_eq!(
        world.powered_cable_anchor_ids(cable),
        [Some(top_anchor), Some(bottom_anchor)]
    );

    let lift = world.place_cargo_lift(TilePos::new(8, 5)).unwrap();
    assert_eq!(world.object(lift).unwrap().size(), [2, 2]);
    assert_eq!(world.cargo_lift_cable(lift), Some(cable));
    assert_eq!(
        world.cargo_lift_direction(lift),
        Some(CargoLiftDirection::Idle)
    );
    assert_eq!(
        world.container(lift).unwrap().slots().len(),
        usize::from(crate::CARGO_LIFT_SLOTS)
    );
    assert!(matches!(
        world.set_tile(9, 5, Layer::Foreground, ForegroundTile::STONE),
        Err(crate::WorldError::OccupiedByObject { object }) if object == lift
    ));
    assert_eq!(
        world.place_cargo_lift(TilePos::new(8, 9)),
        Err(ObjectPlacementError::MissingPoweredCableAttachment(
            TilePos::new(8, 9)
        ))
    );
    assert!(world.place_lift_station(TilePos::new(6, 8)).is_err());
    for x in 6..=7 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let station = world
        .place_furniture(FurnitureObject::LIFT_STATION, TilePos::new(6, 8))
        .unwrap();
    assert_eq!(world.object(station).unwrap().anchor(), TilePos::new(6, 8));
    assert_eq!(world.object(station).unwrap().linked_object(), Some(cable));
    assert_eq!(
        world.lift_station_configuration(station),
        Some(LiftStationConfiguration::DEFAULT)
    );
    assert_eq!(
        world.container(station).unwrap().slots().len(),
        usize::from(crate::LIFT_STATION_SLOTS)
    );
    assert!(world.place_lift_station(TilePos::new(6, 8)).is_err());
    assert!(!world.can_remove_object(cable));
    assert!(!world.can_remove_object(top_anchor));
    assert!(!world.can_remove_object(bottom_anchor));

    assert!(world.remove_object(station).is_some());
    assert!(world.remove_object(lift).is_some());
    assert!(world.remove_object(cable).is_some());
    assert!(world.remove_object(top_anchor).is_some());
    assert!(world.remove_object(bottom_anchor).is_some());
}

#[test]
fn cargo_lift_placement_checks_its_current_footprint_not_the_entire_track() {
    let mut world = World::empty(20, 30, 0).unwrap();
    world
        .set_tile(8, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(8, 3))
        .unwrap();
    for _ in 0..8 {
        world
            .place_or_extend_powered_cable(TilePos::new(8, 3))
            .unwrap();
    }

    // A distant obstruction may stop later travel, but should not prevent the
    // lift from being installed in a clear 2x2 footprint beside the cable.
    world
        .set_tile(9, 10, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let lift = world.place_cargo_lift(TilePos::new(9, 4)).unwrap();

    assert_eq!(world.object(lift).unwrap().anchor(), TilePos::new(9, 4));
    assert_eq!(
        world.cargo_lift_cable(lift),
        world.object_at(TilePos::new(8, 4)).map(WorldObject::id)
    );
}

#[test]
fn orbital_export_launcher_is_three_by_three_with_eight_storage_slots() {
    let mut world = World::empty(10, 10, 0).unwrap();
    for x in 2..=4 {
        world
            .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let id = world
        .place_furniture(FurnitureObject::ORBITAL_EXPORT_LAUNCHER, TilePos::new(2, 4))
        .unwrap();

    assert_eq!(world.object(id).unwrap().size(), [3, 3]);
    assert_eq!(
        world.furniture_interaction_at(TilePos::new(4, 6)),
        Some((
            id,
            FurnitureInteraction::container_with_transport(
                crate::ORBITAL_EXPORT_LAUNCHER_SLOTS,
                ItemTransportRole::Input,
            ),
        ))
    );
    assert_eq!(
        world.container(id).unwrap().slots().len(),
        usize::from(crate::ORBITAL_EXPORT_LAUNCHER_SLOTS)
    );
}

#[test]
fn power_furniture_uses_its_full_floor_supported_footprints() {
    let mut world = World::empty(12, 12, 0).unwrap();
    for x in [2, 3, 6, 8, 9] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 5))
        .unwrap();
    let pylon = world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 6))
        .unwrap();
    let battery = world
        .place_furniture(FurnitureObject::BATTERY, TilePos::new(8, 6))
        .unwrap();

    assert_eq!(world.object(solar).unwrap().size(), [2, 3]);
    assert_eq!(world.object(pylon).unwrap().size(), [1, 2]);
    assert_eq!(world.object(battery).unwrap().size(), [2, 2]);
    assert_eq!(world.object_at(TilePos::new(3, 7)).unwrap().id(), solar);
    assert_eq!(world.object_at(TilePos::new(6, 7)).unwrap().id(), pylon);
    assert_eq!(world.object_at(TilePos::new(9, 7)).unwrap().id(), battery);
    assert_eq!(world.battery_charge_milli(battery), Some(0));
    assert_eq!(
        world.furniture_interaction_at(TilePos::new(2, 5)),
        Some((solar, FurnitureInteraction::NONE))
    );
    assert_eq!(
        world.can_place_furniture(FurnitureObject::PYLON, TilePos::new(10, 6)),
        Err(ObjectPlacementError::RootIsEmpty(TilePos::new(10, 8)))
    );

    world
        .set_tile(3, 8, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert!(world.object(solar).is_none());
    world
        .set_tile(6, 8, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert!(world.object(pylon).is_none());
    world
        .set_tile(8, 8, Layer::Foreground, TileId::EMPTY)
        .unwrap();
    assert!(world.object(battery).is_none());
}

#[test]
fn cargo_conveyors_can_start_beside_terrain_and_connect_later_without_junctions() {
    let mut world = World::empty(14, 14, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 8))
        .unwrap();

    assert_eq!(
        world.can_place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(11, 2)),
        Err(ObjectPlacementError::MissingTransportConnection(
            TilePos::new(11, 2)
        ))
    );
    world
        .set_tile(10, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(11, 2))
        .unwrap();
    world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(11, 3))
        .unwrap();

    let first = world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(4, 9))
        .unwrap();
    world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(5, 9))
        .unwrap();
    world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(6, 9))
        .unwrap();
    world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(6, 8))
        .unwrap();
    world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(6, 7))
        .unwrap();

    assert_eq!(world.object(first).unwrap().root(), TilePos::new(4, 9));
    assert_eq!(world.tile(6, 10, Layer::Foreground).unwrap(), TileId::EMPTY);
    assert_eq!(
        world.can_place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(5, 10)),
        Err(ObjectPlacementError::UnsupportedTransportJunction(
            TilePos::new(5, 9)
        ))
    );
}
