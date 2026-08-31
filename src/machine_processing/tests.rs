use super::*;
use crate::{ForegroundTile, FurnitureObject, Layer, TilePos};

fn powered_world() -> (World, PowerSystem, ObjectId) {
    let mut world = World::empty(32, 16, 7).unwrap();
    for x in [2, 3, 9, 14, 15, 16] {
        world
            .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(9, 10))
        .unwrap();
    let assembler = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(14, 10))
        .unwrap();
    assert!(world.set_furniture_active(assembler, true));
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    assert!(power.is_powered(assembler));
    (world, power, assembler)
}

#[test]
fn powered_assembler_consumes_two_inputs_and_produces_composite() {
    let (mut world, power, assembler) = powered_world();
    let registry = ItemRegistry::with_built_ins();
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 2));
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(1, ItemStack::new(ItemId::STONE_BLOCK, 3));
    let mut system = MachineProcessingSystem::new();

    let update = system.update(&mut world, &registry, &power, COMPOSITE_ASSEMBLY_INTERVAL);

    assert_eq!(update.crafts_completed, 1);
    assert_eq!(
        world.container(assembler).unwrap().slot(0),
        ItemStack::new(ItemId::DIRT_BLOCK, 1)
    );
    assert_eq!(
        world.container(assembler).unwrap().slot(1),
        ItemStack::new(ItemId::STONE_BLOCK, 2)
    );
    assert_eq!(
        world.container(assembler).unwrap().slot(2),
        ItemStack::new(ItemId::HARDENED_COMPOSITE, 1)
    );
}

#[test]
fn logistics_bonus_increases_processing_throughput() {
    let (mut world, power, assembler) = powered_world();
    let registry = ItemRegistry::with_built_ins();
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 1));
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(1, ItemStack::new(ItemId::STONE_BLOCK, 1));
    let mut system = MachineProcessingSystem::new();

    let update = system.update_with_speed(
        &mut world,
        &registry,
        &power,
        Duration::from_millis(800),
        125,
    );

    assert_eq!(update.crafts_completed, 1);
}

#[test]
fn assembler_requires_power_and_both_inputs() {
    let (mut world, _power, assembler) = powered_world();
    let registry = ItemRegistry::with_built_ins();
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 1));
    let mut system = MachineProcessingSystem::new();
    let power = PowerSystem::new();

    assert_eq!(
        system
            .update(&mut world, &registry, &power, COMPOSITE_ASSEMBLY_INTERVAL)
            .crafts_completed,
        0
    );
    assert_eq!(world.container(assembler).unwrap().slot(2), None);
}

#[test]
fn powered_assembler_refines_iron_ore() {
    let (mut world, power, assembler) = powered_world();
    let registry = ItemRegistry::with_built_ins();
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::IRON_ORE, 2));
    let mut system = MachineProcessingSystem::new();

    let update = system.update(&mut world, &registry, &power, COMPOSITE_ASSEMBLY_INTERVAL);

    assert_eq!(update.iron_ore_processed, 1);
    assert_eq!(
        world.container(assembler).unwrap().slot(2),
        ItemStack::new(ItemId::IRON_INGOT, 1)
    );
}
