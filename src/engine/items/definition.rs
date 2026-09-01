use crate::{BackgroundTile, Layer, ObjectTypeId, TileId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct ItemId(u16);

impl ItemId {
    pub const DIRT_BLOCK: Self = Self(1);
    pub const STONE_BLOCK: Self = Self(2);
    pub const RED_LIGHT: Self = Self(3);
    pub const PICKAXE: Self = Self(4);
    pub const HEALING_POTION: Self = Self(5);
    pub const GLOW_STICK: Self = Self(6);
    pub const BOMB: Self = Self(7);
    pub const CHEST: Self = Self(8);
    pub const LASER_BORE: Self = Self(9);
    pub const TURRET: Self = Self(10);
    pub const ORBITAL_EXPORT_LAUNCHER: Self = Self(11);
    pub const CARGO_CONVEYOR: Self = Self(12);
    pub const SOLAR_ARRAY: Self = Self(13);
    pub const PYLON: Self = Self(14);
    pub const BATTERY: Self = Self(15);
    pub const ROPE: Self = Self(16);
    pub const POWERED_CABLE: Self = Self(17);
    pub const POWERED_CABLE_ANCHOR: Self = Self(18);
    pub const CARGO_LIFT: Self = Self(19);
    pub const LIFT_STATION: Self = Self(20);
    pub const POWER_CONNECTOR: Self = Self(21);
    pub const HARDENED_COMPOSITE: Self = Self(22);
    pub const COMPOSITE_ASSEMBLER: Self = Self(23);
    pub const RED_SHAFT_BORE: Self = Self(24);
    pub const PROCUREMENT_TERMINAL: Self = Self(25);
    pub const LASER_DRILL: Self = Self(26);
    pub const AMMO_TURRET: Self = Self(27);
    pub const DIRECTIONAL_SENTRY: Self = Self(28);
    pub const TURRET_AMMO: Self = Self(29);
    pub const SPIKES: Self = Self(30);
    pub const DOOR: Self = Self(31);
    pub const BED: Self = Self(32);
    pub const IRON_ORE: Self = Self(33);
    pub const SUBSURFACE_SURVEYOR: Self = Self(34);
    pub const IRON_INGOT: Self = Self(35);
    pub const ASTERITE: Self = Self(36);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

pub(crate) fn mined_block_drop(tile: TileId, layer: Layer) -> Option<(ItemId, u16)> {
    // Kept as a small function so mining systems do not depend on registry
    // storage details.
    match layer {
        Layer::Foreground => {
            crate::block_definition(tile).and_then(crate::BlockDefinition::mined_drop)
        }
        Layer::Background if tile == BackgroundTile::DIRT_WALL => Some((ItemId::DIRT_BLOCK, 999)),
        Layer::Background if tile == BackgroundTile::STONE_WALL => Some((ItemId::STONE_BLOCK, 999)),
        Layer::Background => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemCategory {
    Block,
    Tool,
    Consumable,
    Furniture,
    Material,
    Custom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolAction {
    RemoveTile { layer: Layer, power: u16 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumableAction {
    Heal { amount: u16 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectileKind {
    GlowStick,
    Bomb,
}

/// Describes what happens when an item is used. New gameplay systems can reserve
/// a `Custom` ID without changing inventory or GUI code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemAction {
    None,
    PlaceTile { layer: Layer, tile: TileId },
    PlaceFurniture { object_type: ObjectTypeId },
    PlaceRope,
    PlacePoweredCable,
    PlaceCargoLift { object_type: ObjectTypeId },
    Tool(ToolAction),
    Consume(ConsumableAction),
    Throw(ProjectileKind),
    Custom(u16),
}

impl ItemAction {
    pub const fn supports_continuous_use(self) -> bool {
        matches!(
            self,
            Self::PlaceTile { .. }
                | Self::PlaceFurniture { .. }
                | Self::PlaceRope
                | Self::PlacePoweredCable
                | Self::PlaceCargoLift { .. }
                | Self::Tool(_)
        )
    }

    pub const fn has_world_target(self) -> bool {
        matches!(
            self,
            Self::PlaceTile { .. }
                | Self::PlaceFurniture { .. }
                | Self::PlaceRope
                | Self::PlacePoweredCable
                | Self::PlaceCargoLift { .. }
                | Self::Tool(_)
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    pub id: ItemId,
    pub name: String,
    pub category: ItemCategory,
    pub max_stack: u16,
    pub icon: u32,
    pub action: ItemAction,
    /// Money paid per item when this definition is exported. Content that does
    /// not opt into the economy remains safely worth zero.
    pub export_value: u64,
}

impl ItemDefinition {
    pub fn new(
        id: ItemId,
        name: impl Into<String>,
        category: ItemCategory,
        max_stack: u16,
        icon: u32,
        action: ItemAction,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            category,
            max_stack,
            icon,
            action,
            export_value: 0,
        }
    }

    pub fn with_export_value(mut self, export_value: u64) -> Self {
        self.export_value = export_value;
        self
    }

    pub fn block(
        id: ItemId,
        name: impl Into<String>,
        icon: u32,
        layer: Layer,
        tile: TileId,
    ) -> Self {
        Self::new(
            id,
            name,
            ItemCategory::Block,
            999,
            icon,
            ItemAction::PlaceTile { layer, tile },
        )
    }
}
