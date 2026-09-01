use super::*;
use crate::{ForegroundTile, FurnitureObject, ItemId, ItemStack, Layer};

fn connected_machines() -> (World, ObjectId, ObjectId) {
    let mut world = World::empty(32, 16, 0).unwrap();
    for x in 1..=12 {
        world
            .set_tile(x, 9, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(1, 6))
        .unwrap();
    for x in 5..=9 {
        world
            .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(x, 8))
            .unwrap();
    }
    let launcher = world
        .place_furniture(
            FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
            TilePos::new(10, 6),
        )
        .unwrap();
    (world, bore, launcher)
}

#[test]
fn connected_drill_output_moves_to_exporter_off_screen() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, bore, launcher) = connected_machines();
    assert!(
        world
            .container_mut(bore)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 24))
    );
    let mut system = ItemTransportSystem::default();

    let initial = system.update(&mut world, &registry, 0.0);
    assert!(initial.topology_rebuilt);
    assert_eq!(initial.connector_count, 5);
    assert_eq!(initial.network_count, 1);
    assert_eq!(initial.transfer_count, 0);

    let moved = system.update(&mut world, &registry, 1.0);
    assert_eq!(moved.transfer_count, 20);
    assert_eq!(moved.items_transferred, 20);
    assert_eq!(
        world.container(bore).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 4)
    );
    assert_eq!(
        world.container(launcher).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 20)
    );
}

#[test]
fn logistics_bonus_increases_transport_tick_rate() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, bore, launcher) = connected_machines();
    world
        .container_mut(bore)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 2));
    let mut system = ItemTransportSystem::default();

    assert_eq!(
        system
            .update_with_speed(&mut world, &registry, 0.025, 100)
            .transfer_count,
        0
    );
    assert_eq!(
        system
            .update_with_speed(&mut world, &registry, 0.0, 200)
            .transfer_count,
        1
    );
    assert_eq!(
        world.container(launcher).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );
}

#[test]
fn input_machines_never_feed_items_backward_into_outputs() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, bore, launcher) = connected_machines();
    assert!(
        world
            .container_mut(launcher)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 8))
    );
    let mut system = ItemTransportSystem::default();
    system.update(&mut world, &registry, 0.0);

    assert_eq!(system.update(&mut world, &registry, 1.0).transfer_count, 0);
    assert!(world.container(bore).unwrap().is_empty());
    assert_eq!(
        world.container(launcher).unwrap().slot(0),
        ItemStack::new(ItemId::DIRT_BLOCK, 8)
    );
}

#[test]
fn a_gap_keeps_endpoints_in_separate_networks() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, bore, launcher) = connected_machines();
    assert!(world.remove_object_at(TilePos::new(7, 8)).is_some());
    assert!(
        world
            .container_mut(bore)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 3))
    );
    let mut system = ItemTransportSystem::default();

    let update = system.update(&mut world, &registry, 1.0);
    assert!(update.topology_rebuilt);
    assert_eq!(update.network_count, 0);
    assert_eq!(update.transfer_count, 0);
    assert!(world.container(launcher).unwrap().is_empty());
}

#[test]
fn empty_buffer_accepts_output_when_no_input_machine_is_connected() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, bore, launcher) = connected_machines();
    assert!(world.remove_object(launcher).is_some());
    for x in 10..=11 {
        world
            .set_tile(x, 9, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(10, 7))
        .unwrap();
    assert!(
        world
            .container_mut(bore)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 6))
    );
    let mut system = ItemTransportSystem::default();

    let update = system.update(&mut world, &registry, 1.0);
    assert_eq!(update.transfer_count, 6);
    assert_eq!(
        world.container(chest).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 6)
    );
}

#[test]
fn buffer_contents_feed_an_input_machine() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, bore, launcher) = connected_machines();
    assert!(world.remove_object(bore).is_some());
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(3, 7))
        .unwrap();
    assert!(
        world
            .container_mut(chest)
            .unwrap()
            .set_slot(4, ItemStack::new(ItemId::DIRT_BLOCK, 17))
    );
    let mut system = ItemTransportSystem::default();

    let update = system.update(&mut world, &registry, 1.0);
    assert_eq!(update.transfer_count, 17);
    assert!(world.container(chest).unwrap().is_empty());
    assert_eq!(
        world.container(launcher).unwrap().slot(0),
        ItemStack::new(ItemId::DIRT_BLOCK, 17)
    );
}

#[test]
fn conveyors_placed_before_endpoints_connect_a_chest_to_an_input() {
    let registry = ItemRegistry::with_built_ins();
    let mut world = World::empty(20, 16, 0).unwrap();
    for x in 2..=12 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in (4..=9).rev() {
        world
            .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(x, 9))
            .unwrap();
    }
    let launcher = world
        .place_furniture(
            FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
            TilePos::new(10, 7),
        )
        .unwrap();
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 8))
        .unwrap();
    assert!(
        world
            .container_mut(chest)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 7))
    );

    let mut system = ItemTransportSystem::default();
    let update = system.update(&mut world, &registry, 1.0);

    assert!(update.topology_rebuilt);
    assert_eq!(update.connector_count, 6);
    assert_eq!(update.network_count, 1);
    assert_eq!(update.transfer_count, 7);
    assert!(world.container(chest).unwrap().is_empty());
    assert_eq!(
        world.container(launcher).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 7)
    );
}

#[test]
fn active_drill_mines_into_a_connected_exporter_without_renderer_state() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, bore, launcher) = connected_machines();
    for x in [15, 18, 19] {
        world
            .set_tile(x, 9, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(2, 10, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(15, 7))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(18, 6))
        .unwrap();
    let mut power = crate::PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert!(world.set_furniture_active(bore, true));
    for _ in 0..3 {
        world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    }
    assert_eq!(
        world.container(bore).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );

    let mut system = ItemTransportSystem::default();
    assert_eq!(system.update(&mut world, &registry, 1.0).transfer_count, 1);
    assert_eq!(
        world.container(launcher).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );
}

#[test]
fn unrelated_object_changes_do_not_rebuild_transport_topology() {
    let registry = ItemRegistry::with_built_ins();
    let (mut world, _, _) = connected_machines();
    let mut system = ItemTransportSystem::default();
    assert!(system.update(&mut world, &registry, 0.0).topology_rebuilt);

    world
        .set_tile(20, 9, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    world
        .place_natural_object(
            crate::NaturalObject::GRASS,
            TilePos::new(20, 8),
            TilePos::new(20, 9),
        )
        .unwrap();
    assert!(!system.update(&mut world, &registry, 0.0).topology_rebuilt);

    world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(11, 5))
        .unwrap();
    assert!(system.update(&mut world, &registry, 0.0).topology_rebuilt);
}

#[test]
fn connector_shape_tracks_straights_and_corners() {
    assert_eq!(
        shape_from_connections(false, true, false, false),
        ItemTransportShape::Horizontal
    );
    assert_eq!(
        shape_from_connections(true, false, true, false),
        ItemTransportShape::Vertical
    );
    assert_eq!(
        shape_from_connections(true, true, false, false),
        ItemTransportShape::NorthEast
    );
    assert_eq!(
        shape_from_connections(false, true, true, false),
        ItemTransportShape::SouthEast
    );
    assert_eq!(
        shape_from_connections(false, false, true, true),
        ItemTransportShape::SouthWest
    );
    assert_eq!(
        shape_from_connections(true, false, false, true),
        ItemTransportShape::NorthWest
    );
}

#[test]
fn vertical_corner_network_transfers_items() {
    let registry = ItemRegistry::with_built_ins();
    let mut world = World::empty(16, 16, 0).unwrap();
    for x in 2..=4 {
        world
            .set_tile(x, 13, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in 8..=10 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let launcher = world
        .place_furniture(
            FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
            TilePos::new(2, 10),
        )
        .unwrap();
    let path = [
        TilePos::new(5, 12),
        TilePos::new(6, 12),
        TilePos::new(7, 12),
        TilePos::new(7, 11),
        TilePos::new(7, 10),
        TilePos::new(7, 9),
        TilePos::new(7, 8),
        TilePos::new(7, 7),
    ];
    let mut connectors = Vec::new();
    for anchor in path {
        connectors.push(
            world
                .place_furniture(FurnitureObject::CARGO_CONVEYOR, anchor)
                .unwrap(),
        );
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(8, 6))
        .unwrap();
    assert!(
        world
            .container_mut(chest)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 9))
    );

    assert_eq!(
        item_transport_shape(&world, connectors[2]),
        Some(ItemTransportShape::NorthWest)
    );
    assert_eq!(
        item_transport_shape(&world, connectors[3]),
        Some(ItemTransportShape::Vertical)
    );

    let mut system = ItemTransportSystem::default();
    assert_eq!(system.update(&mut world, &registry, 1.0).transfer_count, 9);
    assert_eq!(
        world.container(launcher).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 9)
    );
}

#[test]
fn processor_routes_ingredients_into_inputs_and_extracts_only_output() {
    let registry = ItemRegistry::with_built_ins();
    let mut world = World::empty(16, 16, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in 8..=10 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 8))
        .unwrap();
    for x in 4..=7 {
        world
            .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(x, 9))
            .unwrap();
    }
    let assembler = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(8, 8))
        .unwrap();
    world
        .container_mut(chest)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 1));
    world
        .container_mut(chest)
        .unwrap()
        .set_slot(1, ItemStack::new(ItemId::STONE_BLOCK, 1));
    let mut system = ItemTransportSystem::default();

    assert_eq!(system.update(&mut world, &registry, 1.0).transfer_count, 2);
    assert_eq!(
        world.container(assembler).unwrap().slot(0),
        ItemStack::new(ItemId::DIRT_BLOCK, 1)
    );
    assert_eq!(
        world.container(assembler).unwrap().slot(1),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(2, ItemStack::new(ItemId::HARDENED_COMPOSITE, 1));

    assert_eq!(system.update(&mut world, &registry, 0.05).transfer_count, 1);
    assert_eq!(
        world.container(assembler).unwrap().slot(0),
        ItemStack::new(ItemId::DIRT_BLOCK, 1)
    );
    assert_eq!(
        world.container(assembler).unwrap().slot(1),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );
    assert_eq!(world.container(assembler).unwrap().slot(2), None);
    assert!(
        world
            .container(chest)
            .unwrap()
            .slots()
            .iter()
            .flatten()
            .any(|stack| stack.item() == ItemId::HARDENED_COMPOSITE)
    );
}

#[test]
fn processor_inputs_cannot_be_extracted_when_the_output_is_empty() {
    let registry = ItemRegistry::with_built_ins();
    let mut world = World::empty(16, 16, 0).unwrap();
    for x in 2..=10 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let assembler = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(2, 8))
        .unwrap();
    for x in 5..=7 {
        world
            .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(x, 9))
            .unwrap();
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(8, 8))
        .unwrap();
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::IRON_ORE, 8));

    let mut system = ItemTransportSystem::default();
    assert_eq!(system.update(&mut world, &registry, 1.0).transfer_count, 0);
    assert_eq!(
        world.container(assembler).unwrap().slot(0),
        ItemStack::new(ItemId::IRON_ORE, 8)
    );
    assert!(world.container(chest).unwrap().is_empty());
}

#[test]
fn processor_input_items_are_never_extracted_even_from_malformed_output_state() {
    let registry = ItemRegistry::with_built_ins();
    let mut world = World::empty(16, 16, 0).unwrap();
    for x in 2..=10 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let processor = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(2, 8))
        .unwrap();
    for x in 5..=7 {
        world
            .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(x, 9))
            .unwrap();
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(8, 8))
        .unwrap();
    for slot in 0..3 {
        world
            .container_mut(processor)
            .unwrap()
            .set_slot(slot, ItemStack::new(ItemId::IRON_ORE, 8));
    }

    let mut system = ItemTransportSystem::default();
    assert_eq!(system.update(&mut world, &registry, 2.0).transfer_count, 0);
    for slot in 0..3 {
        assert_eq!(
            world.container(processor).unwrap().slot(slot),
            ItemStack::new(ItemId::IRON_ORE, 8)
        );
    }
    assert!(world.container(chest).unwrap().is_empty());
}

#[test]
fn generic_cargo_transfer_cannot_extract_any_processor_input() {
    let registry = ItemRegistry::with_built_ins();
    let mut world = World::empty(16, 16, 0).unwrap();
    for x in 2..=10 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let processor = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(2, 8))
        .unwrap();
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(8, 8))
        .unwrap();
    world
        .container_mut(processor)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::IRON_ORE, 8));

    assert!(!world.transfer_one_container_item(processor, chest, &registry));
    assert_eq!(
        world.container(processor).unwrap().slot(0),
        ItemStack::new(ItemId::IRON_ORE, 8)
    );
    assert!(world.container(chest).unwrap().is_empty());
}
