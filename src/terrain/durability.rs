use super::{
    BackgroundTile, BrokenTile, Layer, TileId, TilePos, World, WorldError, block_definition,
};
use std::collections::HashMap;

/// Health used for foreground content that has no registered block definition.
/// This keeps modded and generated non-empty tiles destructible without adding a
/// dense health plane to every chunk.
pub const DEFAULT_BLOCK_HEALTH: u16 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockHealth {
    current: u16,
    maximum: u16,
}

impl BlockHealth {
    pub const fn current(self) -> u16 {
        self.current
    }

    pub const fn maximum(self) -> u16 {
        self.maximum
    }

    pub const fn damage(self) -> u16 {
        self.maximum - self.current
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockDamage {
    pub applied: u16,
    pub health: Option<BlockHealth>,
    pub broken: Option<BrokenTile>,
}

impl BlockDamage {
    const fn missed() -> Self {
        Self {
            applied: 0,
            health: None,
            broken: None,
        }
    }

    pub const fn is_broken(&self) -> bool {
        self.broken.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct DamagedBlockKey {
    pub position: TilePos,
    pub layer: Layer,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct BlockDamageStore {
    damage: HashMap<DamagedBlockKey, u16>,
}

impl BlockDamageStore {
    fn damage(&self, key: DamagedBlockKey) -> u16 {
        self.damage.get(&key).copied().unwrap_or(0)
    }

    fn set_damage(&mut self, key: DamagedBlockKey, damage: u16) {
        if damage == 0 {
            self.damage.remove(&key);
        } else {
            self.damage.insert(key, damage);
        }
    }

    pub(super) fn remove(&mut self, position: TilePos, layer: Layer) {
        self.damage.remove(&DamagedBlockKey { position, layer });
    }

    pub(super) fn len(&self) -> usize {
        self.damage.len()
    }

    pub(super) fn entries(&self) -> impl Iterator<Item = (DamagedBlockKey, u16)> + '_ {
        self.damage.iter().map(|(&key, &damage)| (key, damage))
    }
}

impl World {
    pub fn block_health(
        &self,
        position: TilePos,
        layer: Layer,
    ) -> Result<Option<BlockHealth>, WorldError> {
        let tile = self.tile(position.x, position.y, layer)?;
        if tile == TileId::EMPTY {
            return Ok(None);
        }
        let maximum = maximum_block_health(tile, layer);
        let damage = self
            .block_damage
            .damage(DamagedBlockKey { position, layer })
            .min(maximum);
        Ok(Some(BlockHealth {
            current: maximum - damage,
            maximum,
        }))
    }

    /// Applies damage in O(1) expected time. Only injured blocks allocate a
    /// sparse record; untouched worlds retain no durability-side storage.
    pub fn damage_block(
        &mut self,
        position: TilePos,
        layer: Layer,
        amount: u16,
    ) -> Result<BlockDamage, WorldError> {
        let Some(health) = self.block_health(position, layer)? else {
            return Ok(BlockDamage::missed());
        };
        if amount == 0 {
            return Ok(BlockDamage {
                applied: 0,
                health: Some(health),
                broken: None,
            });
        }

        let applied = amount.min(health.current);
        if applied == health.current {
            let broken = self.break_tile(position, layer)?;
            return Ok(BlockDamage {
                applied,
                health: None,
                broken,
            });
        }

        let key = DamagedBlockKey { position, layer };
        let damage = self.block_damage.damage(key).saturating_add(applied);
        self.block_damage.set_damage(key, damage);
        Ok(BlockDamage {
            applied,
            health: Some(BlockHealth {
                current: health.current - applied,
                maximum: health.maximum,
            }),
            broken: None,
        })
    }

    pub fn damaged_block_count(&self) -> usize {
        self.block_damage.len()
    }

    pub(super) fn clear_block_damage(&mut self, position: TilePos, layer: Layer) {
        self.block_damage.remove(position, layer);
    }

    pub(super) fn block_damage_entries(&self) -> impl Iterator<Item = (DamagedBlockKey, u16)> + '_ {
        self.block_damage.entries()
    }

    pub(super) fn restore_block_damage(
        &mut self,
        position: TilePos,
        layer: Layer,
        damage: u16,
    ) -> Result<(), WorldError> {
        if position.x >= self.width() || position.y >= self.height() {
            return Err(WorldError::InvalidData(
                "block damage position is outside the world".into(),
            ));
        }
        let Some(health) = self.block_health(position, layer)? else {
            return Err(WorldError::InvalidData(
                "block damage references an empty tile".into(),
            ));
        };
        if damage == 0 || damage >= health.maximum {
            return Err(WorldError::InvalidData(
                "block damage is outside its valid range".into(),
            ));
        }
        let key = DamagedBlockKey { position, layer };
        if self.block_damage.damage.contains_key(&key) {
            return Err(WorldError::InvalidData(
                "block damage contains a duplicate tile".into(),
            ));
        }
        self.block_damage.set_damage(key, damage);
        Ok(())
    }
}

pub(crate) fn maximum_block_health(tile: TileId, layer: Layer) -> u16 {
    match layer {
        Layer::Foreground => block_definition(tile).map_or(DEFAULT_BLOCK_HEALTH, |definition| {
            definition.maximum_health().max(1)
        }),
        Layer::Background if tile == BackgroundTile::DIRT_WALL => 8,
        Layer::Background if tile == BackgroundTile::STONE_WALL => 20,
        Layer::Background => DEFAULT_BLOCK_HEALTH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForegroundTile;

    #[test]
    fn damage_is_sparse_and_accumulates_until_the_block_breaks() {
        let mut world = World::empty(8, 8, 0).unwrap();
        let position = TilePos::new(3, 4);
        world
            .set_tile(
                position.x,
                position.y,
                Layer::Foreground,
                ForegroundTile::STONE,
            )
            .unwrap();

        assert_eq!(world.damaged_block_count(), 0);
        let first = world.damage_block(position, Layer::Foreground, 7).unwrap();
        assert_eq!(first.applied, 7);
        assert_eq!(first.health.unwrap().current(), 33);
        assert!(!first.is_broken());
        assert_eq!(world.damaged_block_count(), 1);

        let final_hit = world
            .damage_block(position, Layer::Foreground, 100)
            .unwrap();
        assert_eq!(final_hit.applied, 33);
        assert_eq!(
            final_hit.broken.as_ref().unwrap().tile,
            ForegroundTile::STONE
        );
        assert_eq!(world.damaged_block_count(), 0);
        assert_eq!(
            world
                .tile(position.x, position.y, Layer::Foreground)
                .unwrap(),
            TileId::EMPTY
        );
    }

    #[test]
    fn replacing_a_tile_clears_its_previous_damage() {
        let mut world = World::empty(8, 8, 0).unwrap();
        let position = TilePos::new(2, 2);
        world
            .set_tile(
                position.x,
                position.y,
                Layer::Foreground,
                ForegroundTile::STONE,
            )
            .unwrap();
        world.damage_block(position, Layer::Foreground, 9).unwrap();

        world
            .set_tile(
                position.x,
                position.y,
                Layer::Foreground,
                ForegroundTile::DIRT,
            )
            .unwrap();

        assert_eq!(world.damaged_block_count(), 0);
        assert_eq!(
            world.block_health(position, Layer::Foreground).unwrap(),
            Some(BlockHealth {
                current: 16,
                maximum: 16
            })
        );
    }
}
