use super::{BackgroundTile, ForegroundTile, TileId};
use crate::items::ItemId;

/// Shared gameplay metadata for a foreground block. Rendering still obtains
/// UVs directly from the stable tile ID, while drops and lighting resolve this
/// table instead of maintaining separate type-specific matches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockDefinition {
    tile: TileId,
    name: &'static str,
    maximum_health: u16,
    background_tile: Option<TileId>,
    mined_drop: Option<(ItemId, u16)>,
    ore_yield: Option<ItemId>,
    emitted_light: Option<[f32; 3]>,
}

impl BlockDefinition {
    pub const fn new(
        tile: TileId,
        name: &'static str,
        maximum_health: u16,
        background_tile: Option<TileId>,
        mined_drop: Option<(ItemId, u16)>,
        emitted_light: Option<[f32; 3]>,
    ) -> Self {
        Self {
            tile,
            name,
            maximum_health,
            background_tile,
            mined_drop,
            ore_yield: None,
            emitted_light,
        }
    }

    pub const fn tile(self) -> TileId {
        self.tile
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn maximum_health(self) -> u16 {
        self.maximum_health
    }

    pub const fn background_tile(self) -> Option<TileId> {
        self.background_tile
    }

    pub const fn mined_drop(self) -> Option<(ItemId, u16)> {
        self.mined_drop
    }

    /// Marks this block as a resource reported by subsurface survey machinery.
    pub const fn with_ore_yield(mut self, item: ItemId) -> Self {
        self.ore_yield = Some(item);
        self
    }

    pub const fn ore_yield(self) -> Option<ItemId> {
        self.ore_yield
    }

    pub const fn emitted_light(self) -> Option<[f32; 3]> {
        self.emitted_light
    }
}

const BLOCK_STACK_LIMIT: u16 = 999;

/// The single table for built-in block drops and emitted light. Atlas frame N
/// corresponds to stable tile ID N, matching the terrain shader convention.
pub const BUILT_IN_BLOCKS: &[BlockDefinition] = &[
    BlockDefinition::new(
        ForegroundTile::GRASS,
        "Grass",
        20,
        Some(BackgroundTile::DIRT_WALL),
        Some((ItemId::DIRT_BLOCK, BLOCK_STACK_LIMIT)),
        None,
    ),
    BlockDefinition::new(
        ForegroundTile::DIRT,
        "Dirt",
        16,
        Some(BackgroundTile::DIRT_WALL),
        Some((ItemId::DIRT_BLOCK, BLOCK_STACK_LIMIT)),
        None,
    ),
    BlockDefinition::new(
        ForegroundTile::STONE,
        "Stone",
        40,
        Some(BackgroundTile::STONE_WALL),
        Some((ItemId::STONE_BLOCK, BLOCK_STACK_LIMIT)),
        None,
    ),
    BlockDefinition::new(
        ForegroundTile::IRON_ORE,
        "Iron Ore",
        50,
        Some(BackgroundTile::STONE_WALL),
        Some((ItemId::IRON_ORE, BLOCK_STACK_LIMIT)),
        None,
    )
    .with_ore_yield(ItemId::IRON_ORE),
    BlockDefinition::new(
        ForegroundTile::ASTERITE,
        "Asterite",
        90,
        Some(BackgroundTile::STONE_WALL),
        Some((ItemId::ASTERITE, BLOCK_STACK_LIMIT)),
        Some([0.15, 0.55, 0.9]),
    )
    .with_ore_yield(ItemId::ASTERITE),
    BlockDefinition::new(
        TileId::new(4),
        "Red Light",
        8,
        None,
        Some((ItemId::RED_LIGHT, BLOCK_STACK_LIMIT)),
        Some([1.0, 0.2, 0.2]),
    ),
    BlockDefinition::new(
        TileId::new(6),
        "Blue Light",
        8,
        None,
        None,
        Some([0.1, 0.4, 0.7]),
    ),
];

pub fn block_definition(tile: TileId) -> Option<BlockDefinition> {
    BUILT_IN_BLOCKS
        .iter()
        .copied()
        .find(|definition| definition.tile == tile)
}

pub fn background_tile_for(foreground: TileId) -> Option<TileId> {
    block_definition(foreground).and_then(BlockDefinition::background_tile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_block_ids_are_unique() {
        for (index, definition) in BUILT_IN_BLOCKS.iter().enumerate() {
            assert!(
                BUILT_IN_BLOCKS[index + 1..]
                    .iter()
                    .all(|other| other.tile != definition.tile)
            );
        }
    }

    #[test]
    fn placeable_blocks_map_to_their_wall_tiles() {
        assert_eq!(
            background_tile_for(ForegroundTile::DIRT),
            Some(BackgroundTile::DIRT_WALL)
        );
        assert_eq!(
            background_tile_for(ForegroundTile::STONE),
            Some(BackgroundTile::STONE_WALL)
        );
        assert_eq!(background_tile_for(TileId::new(4)), None);
    }

    #[test]
    fn grass_is_tougher_than_dirt() {
        let grass = block_definition(ForegroundTile::GRASS).unwrap();
        let dirt = block_definition(ForegroundTile::DIRT).unwrap();
        assert!(grass.maximum_health() > dirt.maximum_health());
        assert_eq!(grass.maximum_health(), 20);
        assert_eq!(dirt.maximum_health(), 16);
    }

    #[test]
    fn iron_is_registered_as_a_surveyable_ore() {
        let iron = block_definition(ForegroundTile::IRON_ORE).unwrap();
        assert_eq!(iron.name(), "Iron Ore");
        assert_eq!(iron.ore_yield(), Some(ItemId::IRON_ORE));
        assert_eq!(iron.mined_drop(), Some((ItemId::IRON_ORE, 999)));
        assert!(
            iron.maximum_health()
                > block_definition(ForegroundTile::STONE)
                    .unwrap()
                    .maximum_health()
        );
    }

    #[test]
    fn asterite_is_a_durable_luminous_ore() {
        let asterite = block_definition(ForegroundTile::ASTERITE).unwrap();
        assert_eq!(asterite.ore_yield(), Some(ItemId::ASTERITE));
        assert_eq!(asterite.mined_drop(), Some((ItemId::ASTERITE, 999)));
        assert!(asterite.emitted_light().is_some());
        assert!(
            asterite.maximum_health()
                > block_definition(ForegroundTile::IRON_ORE)
                    .unwrap()
                    .maximum_health()
        );
    }
}
