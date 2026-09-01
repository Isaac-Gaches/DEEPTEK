use crate::{FurnitureObject, ItemRegistry, ItemStack, ObjectId, PowerSystem, World};
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_ORBITAL_EXPORT_INTERVAL: Duration = Duration::from_secs(4);

/// One complete stack removed from an orbital export launcher's inventory.
/// Economy and contract systems consume these events without being coupled to
/// furniture storage or timing internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportShipment {
    pub launcher: ObjectId,
    pub stack: ItemStack,
    pub unit_value: u64,
    pub proceeds: u64,
}

#[derive(Debug)]
pub struct OrbitalExportSystem {
    interval_seconds: f64,
    clock_seconds: f64,
    next_launch: HashMap<ObjectId, f64>,
    launcher_ids: Vec<ObjectId>,
    shipments: Vec<ExportShipment>,
}

impl OrbitalExportSystem {
    pub fn new(interval: Duration) -> Self {
        assert!(
            !interval.is_zero(),
            "orbital export interval must be non-zero"
        );
        Self {
            interval_seconds: interval.as_secs_f64(),
            clock_seconds: 0.0,
            next_launch: HashMap::new(),
            launcher_ids: Vec::new(),
            shipments: Vec::new(),
        }
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs_f64(self.interval_seconds)
    }

    /// Advances all launchers using the object type index rather than scanning
    /// the world. Each ready launcher removes at most one whole stack.
    pub fn update(
        &mut self,
        world: &mut World,
        registry: &ItemRegistry,
        power: &PowerSystem,
        elapsed_seconds: f32,
    ) -> &[ExportShipment] {
        self.shipments.clear();
        if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            self.clock_seconds += f64::from(elapsed_seconds);
        }

        self.launcher_ids.clear();
        self.launcher_ids.extend(
            world
                .objects_of_type(FurnitureObject::ORBITAL_EXPORT_LAUNCHER)
                .filter(|object| object.is_active())
                .map(|object| object.id()),
        );
        self.next_launch.retain(|id, _| {
            world.object(*id).is_some_and(|object| {
                object.object_type() == FurnitureObject::ORBITAL_EXPORT_LAUNCHER
                    && object.is_active()
            })
        });

        for launcher in self.launcher_ids.iter().copied() {
            let next_launch = self
                .next_launch
                .entry(launcher)
                .or_insert(self.clock_seconds + self.interval_seconds);
            if self.clock_seconds < *next_launch {
                continue;
            }
            *next_launch = self.clock_seconds + self.interval_seconds;

            if !power.is_powered(launcher) || !world.orbital_export_has_sky_access(launcher) {
                continue;
            }

            let Some(stack) = world
                .container_mut(launcher)
                .and_then(crate::ItemContainer::take_first_stack)
            else {
                continue;
            };
            let unit_value = registry
                .get(stack.item())
                .map_or(0, |definition| definition.export_value);
            self.shipments.push(ExportShipment {
                launcher,
                stack,
                unit_value,
                proceeds: unit_value.saturating_mul(u64::from(stack.quantity())),
            });
        }
        &self.shipments
    }
}

impl Default for OrbitalExportSystem {
    fn default() -> Self {
        Self::new(DEFAULT_ORBITAL_EXPORT_INTERVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForegroundTile, ItemId, Layer, TilePos};

    fn launcher_world() -> (World, ObjectId, PowerSystem) {
        let mut world = World::empty(12, 120, 0).unwrap();
        for x in 3..=10 {
            world
                .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let launcher = world
            .place_furniture(FurnitureObject::ORBITAL_EXPORT_LAUNCHER, TilePos::new(3, 9))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(8, 10))
            .unwrap();
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(9, 9))
            .unwrap();
        let mut power = PowerSystem::new();
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        (world, launcher, power)
    }

    #[test]
    fn launcher_waits_then_exports_one_complete_stack_per_interval() {
        let registry = ItemRegistry::with_built_ins();
        let (mut world, launcher, power) = launcher_world();
        let container = world.container_mut(launcher).unwrap();
        assert!(container.set_slot(1, ItemStack::new(ItemId::DIRT_BLOCK, 12)));
        assert!(container.set_slot(5, ItemStack::new(ItemId::STONE_BLOCK, 7)));
        let mut system = OrbitalExportSystem::default();

        assert!(system.update(&mut world, &registry, &power, 0.0).is_empty());
        assert!(system.update(&mut world, &registry, &power, 3.0).is_empty());
        let first = system.update(&mut world, &registry, &power, 1.0);
        assert_eq!(first.len(), 1);
        assert_eq!(
            first[0].stack,
            ItemStack::new(ItemId::DIRT_BLOCK, 12).unwrap()
        );
        assert_eq!(first[0].unit_value, 1);
        assert_eq!(first[0].proceeds, 12);
        assert_eq!(world.container(launcher).unwrap().slot(1), None);
        assert_eq!(
            world.container(launcher).unwrap().slot(5),
            ItemStack::new(ItemId::STONE_BLOCK, 7)
        );

        assert!(system.update(&mut world, &registry, &power, 3.0).is_empty());
        let second = system.update(&mut world, &registry, &power, 1.0);
        assert_eq!(second.len(), 1);
        assert_eq!(
            second[0].stack,
            ItemStack::new(ItemId::STONE_BLOCK, 7).unwrap()
        );
        assert_eq!(second[0].proceeds, 28);
        assert!(world.container(launcher).unwrap().is_empty());
    }

    #[test]
    fn missing_item_definitions_still_export_for_zero_money() {
        let registry = ItemRegistry::new();
        let (mut world, launcher, power) = launcher_world();
        assert!(
            world
                .container_mut(launcher)
                .unwrap()
                .set_slot(0, ItemStack::new(ItemId::new(999), 3))
        );
        let mut system = OrbitalExportSystem::new(Duration::from_secs(1));

        assert!(system.update(&mut world, &registry, &power, 0.0).is_empty());
        let shipments = system.update(&mut world, &registry, &power, 1.0);
        assert_eq!(shipments.len(), 1);
        assert_eq!(shipments[0].unit_value, 0);
        assert_eq!(shipments[0].proceeds, 0);
    }

    #[test]
    fn unpowered_launcher_keeps_its_inventory() {
        let registry = ItemRegistry::with_built_ins();
        let (mut world, launcher, _) = launcher_world();
        let solar = world
            .objects_of_type(FurnitureObject::SOLAR_ARRAY)
            .next()
            .unwrap()
            .id();
        assert!(world.remove_object(solar).is_some());
        assert!(
            world
                .container_mut(launcher)
                .unwrap()
                .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 4))
        );
        let mut power = PowerSystem::new();
        power.update(&world);
        let mut system = OrbitalExportSystem::new(Duration::from_secs(1));

        assert!(system.update(&mut world, &registry, &power, 0.0).is_empty());
        assert!(system.update(&mut world, &registry, &power, 1.0).is_empty());
        assert_eq!(
            world.container(launcher).unwrap().slot(0),
            ItemStack::new(ItemId::STONE_BLOCK, 4)
        );
    }

    #[test]
    fn roofed_launcher_keeps_its_inventory() {
        let registry = ItemRegistry::with_built_ins();
        let (mut world, launcher, mut power) = launcher_world();
        assert!(
            world
                .container_mut(launcher)
                .unwrap()
                .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 4))
        );
        world
            .set_tile(4, 4, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        let mut system = OrbitalExportSystem::new(Duration::from_secs(1));

        assert!(system.update(&mut world, &registry, &power, 0.0).is_empty());
        assert!(system.update(&mut world, &registry, &power, 1.0).is_empty());
        assert_eq!(
            world.container(launcher).unwrap().slot(0),
            ItemStack::new(ItemId::STONE_BLOCK, 4)
        );
    }

    #[test]
    fn launcher_below_minus_one_hundred_metres_has_no_sky_access() {
        let mut world = World::empty(12, 300, 0).unwrap();
        for x in 3..=5 {
            world
                .set_tile(x, 190, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let launcher = world
            .place_furniture(
                FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
                TilePos::new(3, 187),
            )
            .unwrap();

        assert!(world.elevation_decimetres(189.0) < -1_000);
        assert!(!world.orbital_export_has_sky_access(launcher));
    }
}
