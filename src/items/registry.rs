use super::{
    ConsumableAction, ItemAction, ItemCategory, ItemDefinition, ItemId, ProjectileKind, ToolAction,
};
use crate::{ForegroundTile, FurnitureObject, Layer, TileId};
use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Default)]
pub struct ItemRegistry {
    definitions: Vec<Option<ItemDefinition>>,
}

impl ItemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_built_ins() -> Self {
        let mut registry = Self::new();
        for definition in built_in_item_definitions() {
            registry
                .register(definition)
                .expect("built-in item definitions are valid");
        }
        registry
    }

    pub fn register(&mut self, definition: ItemDefinition) -> Result<(), ItemRegistryError> {
        if definition.max_stack == 0 {
            return Err(ItemRegistryError::ZeroStackLimit(definition.id));
        }
        let index = usize::from(definition.id.raw());
        if self.definitions.len() <= index {
            self.definitions.resize_with(index + 1, || None);
        }
        if self.definitions[index].is_some() {
            return Err(ItemRegistryError::DuplicateId(definition.id));
        }
        self.definitions[index] = Some(definition);
        Ok(())
    }

    pub fn get(&self, id: ItemId) -> Option<&ItemDefinition> {
        self.definitions
            .get(usize::from(id.raw()))
            .and_then(Option::as_ref)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &ItemDefinition> {
        self.definitions.iter().filter_map(Option::as_ref)
    }

    pub fn item_for_furniture(&self, object_type: crate::ObjectTypeId) -> Option<ItemId> {
        self.definitions().find_map(|definition| {
            let places_object = match definition.action {
                ItemAction::PlaceFurniture {
                    object_type: candidate,
                } => candidate == object_type,
                ItemAction::PlaceCargoLift => object_type == FurnitureObject::CARGO_LIFT,
                _ => false,
            };
            places_object.then_some(definition.id)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemRegistryError {
    DuplicateId(ItemId),
    ZeroStackLimit(ItemId),
}

impl fmt::Display for ItemRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => {
                write!(formatter, "item ID {} is already registered", id.raw())
            }
            Self::ZeroStackLimit(id) => {
                write!(formatter, "item ID {} has a zero stack limit", id.raw())
            }
        }
    }
}

impl Error for ItemRegistryError {}

/// Definitions installed by `ItemRegistry::with_built_ins`. Keeping this as a
/// public constructor makes a game's content bootstrap explicit and testable.
pub fn built_in_item_definitions() -> [ItemDefinition; 21] {
    [
        ItemDefinition::block(
            ItemId::DIRT_BLOCK,
            "Dirt Block",
            5,
            Layer::Foreground,
            ForegroundTile::DIRT,
        )
        .with_export_value(1),
        ItemDefinition::block(
            ItemId::STONE_BLOCK,
            "Stone Block",
            0,
            Layer::Foreground,
            ForegroundTile::STONE,
        )
        .with_export_value(4),
        ItemDefinition::block(
            ItemId::RED_LIGHT,
            "Red Light",
            4,
            Layer::Foreground,
            TileId::new(4),
        )
        .with_export_value(15),
        ItemDefinition::new(
            ItemId::PICKAXE,
            "Copper Pickaxe",
            ItemCategory::Tool,
            1,
            1,
            ItemAction::Tool(ToolAction::RemoveTile {
                layer: Layer::Foreground,
                power: 1,
            }),
        )
        .with_export_value(40),
        ItemDefinition::new(
            ItemId::HEALING_POTION,
            "Healing Potion",
            ItemCategory::Consumable,
            30,
            3,
            ItemAction::Consume(ConsumableAction::Heal { amount: 25 }),
        )
        .with_export_value(30),
        ItemDefinition::new(
            ItemId::GLOW_STICK,
            "Glow Stick",
            ItemCategory::Consumable,
            999,
            1,
            ItemAction::Throw(ProjectileKind::GlowStick),
        )
        .with_export_value(2),
        ItemDefinition::new(
            ItemId::BOMB,
            "Bomb",
            ItemCategory::Consumable,
            999,
            0,
            ItemAction::Throw(ProjectileKind::Bomb),
        )
        .with_export_value(25),
        ItemDefinition::new(
            ItemId::CHEST,
            "Wooden Chest",
            ItemCategory::Furniture,
            99,
            6,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::CHEST,
            },
        )
        .with_export_value(120),
        ItemDefinition::new(
            ItemId::LASER_BORE,
            "Laser Bore",
            ItemCategory::Furniture,
            99,
            7,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::LASER_BORE,
            },
        )
        .with_export_value(750),
        ItemDefinition::new(
            ItemId::TURRET,
            "Defence Turret",
            ItemCategory::Furniture,
            99,
            8,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::TURRET,
            },
        )
        .with_export_value(900),
        ItemDefinition::new(
            ItemId::ORBITAL_EXPORT_LAUNCHER,
            "Orbital Export Launcher",
            ItemCategory::Furniture,
            99,
            9,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
            },
        )
        .with_export_value(1_500),
        ItemDefinition::new(
            ItemId::CARGO_CONVEYOR,
            "Cargo Conveyor",
            ItemCategory::Furniture,
            999,
            10,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::CARGO_CONVEYOR,
            },
        )
        .with_export_value(5),
        ItemDefinition::new(
            ItemId::SOLAR_ARRAY,
            "Solar Array",
            ItemCategory::Furniture,
            99,
            11,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::SOLAR_ARRAY,
            },
        )
        .with_export_value(1_200),
        ItemDefinition::new(
            ItemId::PYLON,
            "Pylon",
            ItemCategory::Furniture,
            999,
            12,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::PYLON,
            },
        )
        .with_export_value(100),
        ItemDefinition::new(
            ItemId::BATTERY,
            "Battery",
            ItemCategory::Furniture,
            99,
            13,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::BATTERY,
            },
        )
        .with_export_value(1_000),
        ItemDefinition::new(
            ItemId::ROPE,
            "Rope",
            ItemCategory::Material,
            999,
            14,
            ItemAction::PlaceRope,
        )
        .with_export_value(2),
        ItemDefinition::new(
            ItemId::POWERED_CABLE,
            "Powered Cable",
            ItemCategory::Material,
            999,
            15,
            ItemAction::PlacePoweredCable,
        )
        .with_export_value(8),
        ItemDefinition::new(
            ItemId::POWERED_CABLE_ANCHOR,
            "Powered Cable Anchor",
            ItemCategory::Furniture,
            99,
            16,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::POWERED_CABLE_ANCHOR,
            },
        )
        .with_export_value(120),
        ItemDefinition::new(
            ItemId::CARGO_LIFT,
            "Cargo Lift",
            ItemCategory::Furniture,
            10,
            17,
            ItemAction::PlaceCargoLift,
        )
        .with_export_value(1_200),
        ItemDefinition::new(
            ItemId::LIFT_STATION,
            "Lift Station",
            ItemCategory::Furniture,
            99,
            18,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::LIFT_STATION,
            },
        )
        .with_export_value(650),
        ItemDefinition::new(
            ItemId::POWER_CONNECTOR,
            "Power Connector",
            ItemCategory::Furniture,
            999,
            12,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::POWER_CONNECTOR,
            },
        )
        .with_export_value(40),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_rejects_duplicate_and_zero_stack_definitions() {
        let mut registry = ItemRegistry::new();
        let definition = ItemDefinition::new(
            ItemId::new(20),
            "Test",
            ItemCategory::Material,
            10,
            0,
            ItemAction::None,
        );
        registry.register(definition.clone()).unwrap();
        assert_eq!(
            registry.register(definition),
            Err(ItemRegistryError::DuplicateId(ItemId::new(20)))
        );
        assert_eq!(
            registry.register(ItemDefinition::new(
                ItemId::new(21),
                "Broken",
                ItemCategory::Material,
                0,
                0,
                ItemAction::None,
            )),
            Err(ItemRegistryError::ZeroStackLimit(ItemId::new(21)))
        );
    }

    #[test]
    fn export_value_defaults_to_zero_and_can_be_set_by_content() {
        let default = ItemDefinition::new(
            ItemId::new(30),
            "Unpriced",
            ItemCategory::Custom,
            1,
            0,
            ItemAction::None,
        );
        assert_eq!(default.export_value, 0);
        assert_eq!(default.with_export_value(75).export_value, 75);
    }

    #[test]
    fn rope_and_furniture_support_held_placement() {
        let registry = ItemRegistry::with_built_ins();
        let rope = registry.get(ItemId::ROPE).unwrap();
        let chest = registry.get(ItemId::CHEST).unwrap();

        assert_eq!(rope.name, "Rope");
        assert_eq!(rope.max_stack, 999);
        assert!(matches!(rope.action, ItemAction::PlaceRope));
        assert!(rope.action.supports_continuous_use());
        assert!(matches!(chest.action, ItemAction::PlaceFurniture { .. }));
        assert!(chest.action.supports_continuous_use());

        let powered_cable = registry.get(ItemId::POWERED_CABLE).unwrap();
        let anchor = registry.get(ItemId::POWERED_CABLE_ANCHOR).unwrap();
        let lift = registry.get(ItemId::CARGO_LIFT).unwrap();
        let station = registry.get(ItemId::LIFT_STATION).unwrap();
        let connector = registry.get(ItemId::POWER_CONNECTOR).unwrap();
        assert!(matches!(
            powered_cable.action,
            ItemAction::PlacePoweredCable
        ));
        assert!(powered_cable.action.supports_continuous_use());
        assert!(matches!(anchor.action, ItemAction::PlaceFurniture { .. }));
        assert!(matches!(lift.action, ItemAction::PlaceCargoLift));
        assert!(matches!(
            station.action,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::LIFT_STATION
            }
        ));
        assert!(matches!(
            connector.action,
            ItemAction::PlaceFurniture {
                object_type: FurnitureObject::POWER_CONNECTOR
            }
        ));
    }
}
