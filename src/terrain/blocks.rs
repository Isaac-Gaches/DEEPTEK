use super::{ForegroundTile, TileId};
use crate::items::ItemId;

/// Shared gameplay metadata for a foreground block. Rendering still obtains
/// UVs directly from the stable tile ID, while drops and lighting resolve this
/// table instead of maintaining separate type-specific matches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockDefinition {
    tile: TileId,
    name: &'static str,
    mined_drop: Option<(ItemId, u16)>,
    emitted_light: Option<[f32; 3]>,
}

impl BlockDefinition {
    pub const fn new(
        tile: TileId,
        name: &'static str,
        mined_drop: Option<(ItemId, u16)>,
        emitted_light: Option<[f32; 3]>,
    ) -> Self {
        Self {
            tile,
            name,
            mined_drop,
            emitted_light,
        }
    }

    pub const fn tile(self) -> TileId {
        self.tile
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn mined_drop(self) -> Option<(ItemId, u16)> {
        self.mined_drop
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
        Some((ItemId::DIRT_BLOCK, BLOCK_STACK_LIMIT)),
        None,
    ),
    BlockDefinition::new(
        ForegroundTile::DIRT,
        "Dirt",
        Some((ItemId::DIRT_BLOCK, BLOCK_STACK_LIMIT)),
        None,
    ),
    BlockDefinition::new(
        ForegroundTile::STONE,
        "Stone",
        Some((ItemId::STONE_BLOCK, BLOCK_STACK_LIMIT)),
        None,
    ),
    BlockDefinition::new(
        TileId::new(4),
        "Red Light",
        Some((ItemId::RED_LIGHT, BLOCK_STACK_LIMIT)),
        Some([1.0, 0.2, 0.2]),
    ),
    BlockDefinition::new(TileId::new(6), "Blue Light", None, Some([0.1, 0.4, 0.7])),
];

pub fn block_definition(tile: TileId) -> Option<BlockDefinition> {
    BUILT_IN_BLOCKS
        .iter()
        .copied()
        .find(|definition| definition.tile == tile)
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
}
