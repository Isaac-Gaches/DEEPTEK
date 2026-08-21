use super::objects::{DecorationUpdate, ObjectId, ObjectTypeId, TilePos, WorldObject};
use super::{
    CHUNK_SIZE, FurnitureObject, LASER_BORE_TICKS_PER_TILE, Layer, NaturalObject, TileId, World,
};
use crate::PowerSystem;
use crate::items::mined_block_drop;
use std::time::Duration;

const SIMULATION_TICK_NANOS: u64 = 1_000_000_000;
const GRASS_SCATTER_DIVISOR: u64 = 4;
const GRASS_PEBBLE_SCATTER_DIVISOR: u64 = 16;
const PEBBLE_SCATTER_DIVISOR: u64 = 64;
pub const MAX_VINE_LENGTH: u16 = 10;

enum ScheduledObjectUpdate {
    Unchanged,
    Grown,
    TileDestroyed(TilePos),
}

/// Bounded rates for world-object and active-area natural simulation. Scheduled
/// object events are global and use `object_update_budget`; player-relative radii
/// constrain only column scans for spawning and spreading. A chance divisor of one
/// means every valid candidate succeeds; larger divisors make that event rarer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NatureSimulationConfig {
    pub horizontal_radius_tiles: u32,
    pub vertical_radius_tiles: u32,
    pub columns_per_tick: usize,
    pub max_columns_per_update: usize,
    pub object_update_budget: usize,
    pub grass_spawn_chance: u32,
    pub vine_spawn_chance: u32,
    pub grass_spread_chance: u32,
}

impl Default for NatureSimulationConfig {
    fn default() -> Self {
        Self {
            horizontal_radius_tiles: CHUNK_SIZE as u32 * 6,
            vertical_radius_tiles: CHUNK_SIZE as u32 * 4,
            columns_per_tick: 8,
            max_columns_per_update: 32,
            object_update_budget: 512,
            grass_spawn_chance: 3,
            vine_spawn_chance: 2,
            grass_spread_chance: 4,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NatureUpdate {
    pub decorations: DecorationUpdate,
    pub columns_scanned: usize,
    pub grass_spawned: usize,
    pub vines_spawned: usize,
    pub grass_tiles_spread: usize,
    changed_tiles: Vec<TilePos>,
}

impl NatureUpdate {
    pub fn changed_tiles(&self) -> &[TilePos] {
        &self.changed_tiles
    }
}

impl World {
    pub fn update_decorations(
        &mut self,
        elapsed: Duration,
        update_budget: usize,
    ) -> DecorationUpdate {
        self.update_decorations_internal(elapsed, update_budget, None)
    }

    pub fn update_decorations_with_power(
        &mut self,
        elapsed: Duration,
        update_budget: usize,
        power: &PowerSystem,
    ) -> DecorationUpdate {
        self.update_decorations_internal(elapsed, update_budget, Some(power))
    }

    fn update_decorations_internal(
        &mut self,
        elapsed: Duration,
        update_budget: usize,
        power: Option<&PowerSystem>,
    ) -> DecorationUpdate {
        let elapsed_nanos = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.simulation_remainder_nanos = self
            .simulation_remainder_nanos
            .saturating_add(elapsed_nanos);
        let ticks = self.simulation_remainder_nanos / SIMULATION_TICK_NANOS;
        self.simulation_remainder_nanos %= SIMULATION_TICK_NANOS;
        self.simulation_tick = self.simulation_tick.saturating_add(ticks);

        let mut update = DecorationUpdate {
            ticks_advanced: ticks,
            ..DecorationUpdate::default()
        };
        while update.objects_processed < update_budget {
            let Some(event) = self.objects.pop_due(self.simulation_tick) else {
                break;
            };
            update.objects_processed += 1;
            match self.update_scheduled_object(event.object, power) {
                ScheduledObjectUpdate::Unchanged => {}
                ScheduledObjectUpdate::Grown => update.objects_grown += 1,
                ScheduledObjectUpdate::TileDestroyed(position) => {
                    update.tiles_destroyed += 1;
                    update.changed_tiles.push(position);
                }
            }
        }
        update.budget_exhausted = self.objects.has_due(self.simulation_tick);
        update
    }

    /// Advances globally scheduled object events, including laser bores, before
    /// performing bounded natural simulation around `active_position`. Only the
    /// spawning/spreading scan is player-relative; scheduled work is pulled from a
    /// min-heap and therefore remains active off-screen without scanning the world.
    pub fn update_nature(
        &mut self,
        elapsed: Duration,
        active_position: TilePos,
        config: NatureSimulationConfig,
    ) -> NatureUpdate {
        self.update_nature_internal(elapsed, active_position, config, None)
    }

    pub fn update_nature_with_power(
        &mut self,
        elapsed: Duration,
        active_position: TilePos,
        config: NatureSimulationConfig,
        power: &PowerSystem,
    ) -> NatureUpdate {
        self.update_nature_internal(elapsed, active_position, config, Some(power))
    }

    fn update_nature_internal(
        &mut self,
        elapsed: Duration,
        active_position: TilePos,
        config: NatureSimulationConfig,
        power: Option<&PowerSystem>,
    ) -> NatureUpdate {
        let decorations =
            self.update_decorations_internal(elapsed, config.object_update_budget, power);
        let changed_tiles = decorations.changed_tiles().to_vec();
        let ticks_advanced = decorations.ticks_advanced;
        let mut update = NatureUpdate {
            decorations,
            changed_tiles,
            ..NatureUpdate::default()
        };
        if ticks_advanced == 0 || config.columns_per_tick == 0 || config.max_columns_per_update == 0
        {
            return update;
        }

        let active_position = TilePos {
            x: active_position.x.min(self.width - 1),
            y: active_position.y.min(self.height - 1),
        };

        let min_x = active_position
            .x
            .saturating_sub(config.horizontal_radius_tiles);
        let max_x = active_position
            .x
            .saturating_add(config.horizontal_radius_tiles)
            .min(self.width - 1);
        let min_y = active_position
            .y
            .saturating_sub(config.vertical_radius_tiles);
        let max_y = active_position
            .y
            .saturating_add(config.vertical_radius_tiles)
            .min(self.height - 1);
        let active_width = u64::from(max_x - min_x + 1);
        let columns_per_tick = config
            .columns_per_tick
            .min(active_width as usize)
            .min(config.max_columns_per_update);
        let ticks_allowed = config
            .max_columns_per_update
            .div_ceil(columns_per_tick)
            .min(ticks_advanced as usize);
        let first_tick = self
            .simulation_tick
            .saturating_sub(ticks_allowed as u64)
            .saturating_add(1);

        for tick in first_tick..=self.simulation_tick {
            let remaining = config.max_columns_per_update - update.columns_scanned;
            let column_count = columns_per_tick.min(remaining);
            if column_count == 0 {
                break;
            }
            let start = nature_hash(self.seed, tick, active_position.x, active_position.y, 0)
                % active_width;
            for offset in 0..column_count {
                let x = min_x + ((start + offset as u64) % active_width) as u32;
                self.scan_nature_column(x, min_y, max_y, tick, config, &mut update);
                update.columns_scanned += 1;
            }
        }
        update
    }

    fn scan_nature_column(
        &mut self,
        x: u32,
        min_y: u32,
        max_y: u32,
        tick: u64,
        config: NatureSimulationConfig,
        update: &mut NatureUpdate,
    ) {
        for y in min_y..=max_y {
            let tile = self.tile_in_bounds(x, y, Layer::Foreground);
            if tile == TileId::EMPTY {
                if self.objects.occupying(TilePos { x, y }).is_some() {
                    continue;
                }
                let roll = nature_hash(self.seed, tick, x, y, 1);
                if y > 0
                    && self.tile_in_bounds(x, y - 1, Layer::Foreground)
                        == super::ForegroundTile::GRASS
                    && roll.is_multiple_of(u64::from(config.vine_spawn_chance.max(1)))
                {
                    if self
                        .place_natural_object(
                            NaturalObject::VINE,
                            TilePos { x, y },
                            TilePos { x, y: y - 1 },
                        )
                        .is_ok()
                    {
                        update.vines_spawned += 1;
                    }
                } else if y + 1 < self.height
                    && self.tile_in_bounds(x, y + 1, Layer::Foreground)
                        == super::ForegroundTile::GRASS
                    && roll.is_multiple_of(u64::from(config.grass_spawn_chance.max(1)))
                    && self
                        .place_natural_object(
                            NaturalObject::GRASS,
                            TilePos { x, y },
                            TilePos { x, y: y + 1 },
                        )
                        .is_ok()
                {
                    update.grass_spawned += 1;
                }
            } else if tile == super::ForegroundTile::DIRT {
                let roll = nature_hash(self.seed, tick, x, y, 2);
                if roll.is_multiple_of(u64::from(config.grass_spread_chance.max(1)))
                    && self.dirt_can_grow_grass(x, y)
                {
                    self.set_tile(x, y, Layer::Foreground, super::ForegroundTile::GRASS)
                        .expect("nature scan coordinates are in bounds");
                    update.grass_tiles_spread += 1;
                    update.changed_tiles.push(TilePos { x, y });
                }
            }
        }
    }

    fn dirt_can_grow_grass(&self, x: u32, y: u32) -> bool {
        const CARDINALS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
        let adjacent_grass = CARDINALS.into_iter().any(|(dx, dy)| {
            x.checked_add_signed(dx)
                .zip(y.checked_add_signed(dy))
                .filter(|&(sample_x, sample_y)| sample_x < self.width && sample_y < self.height)
                .is_some_and(|(sample_x, sample_y)| {
                    self.tile_in_bounds(sample_x, sample_y, Layer::Foreground)
                        == super::ForegroundTile::GRASS
                })
        });
        adjacent_grass
            && (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    (dx != 0 || dy != 0)
                        && x.checked_add_signed(dx)
                            .zip(y.checked_add_signed(dy))
                            .filter(|&(sample_x, sample_y)| {
                                sample_x < self.width && sample_y < self.height
                            })
                            .is_some_and(|(sample_x, sample_y)| {
                                self.tile_in_bounds(sample_x, sample_y, Layer::Foreground)
                                    == TileId::EMPTY
                            })
                })
            })
    }

    fn update_scheduled_object(
        &mut self,
        id: ObjectId,
        power: Option<&PowerSystem>,
    ) -> ScheduledObjectUpdate {
        let Some(object) = self.objects.object(id).cloned() else {
            return ScheduledObjectUpdate::Unchanged;
        };
        match object.object_type {
            NaturalObject::GRASS => {
                if object.growth_stage >= 1 {
                    self.objects.schedule(id, u64::MAX);
                    return ScheduledObjectUpdate::Unchanged;
                }
                self.objects.object_mut(id).unwrap().growth_stage += 1;
                self.objects.mark_changed();
                self.objects.schedule(id, u64::MAX);
                ScheduledObjectUpdate::Grown
            }
            NaturalObject::VINE => {
                if object.height >= MAX_VINE_LENGTH {
                    self.objects.schedule(id, u64::MAX);
                    return ScheduledObjectUpdate::Unchanged;
                }
                let next_cell = TilePos {
                    x: object.anchor.x,
                    y: object.anchor.y + u32::from(object.height),
                };
                let can_grow = next_cell.y < self.height
                    && self.tile_in_bounds(next_cell.x, next_cell.y, Layer::Foreground)
                        == TileId::EMPTY
                    && self.objects.occupying(next_cell).is_none();
                let grew = can_grow && self.objects.extend_down(id);
                let delay = growth_delay(self.seed, id, self.simulation_tick, 3, 8);
                self.objects
                    .schedule(id, self.simulation_tick.saturating_add(delay));
                if grew {
                    ScheduledObjectUpdate::Grown
                } else {
                    ScheduledObjectUpdate::Unchanged
                }
            }
            FurnitureObject::LASER_BORE => {
                self.update_laser_bore(id, &object, power.is_some_and(|power| power.is_powered(id)))
            }
            _ => {
                self.objects.schedule(id, u64::MAX);
                ScheduledObjectUpdate::Unchanged
            }
        }
    }

    fn update_laser_bore(
        &mut self,
        id: ObjectId,
        object: &WorldObject,
        powered: bool,
    ) -> ScheduledObjectUpdate {
        if !object.active {
            self.objects.schedule(id, u64::MAX);
            return ScheduledObjectUpdate::Unchanged;
        }
        if !powered {
            self.objects
                .schedule(id, self.simulation_tick.saturating_add(1));
            return ScheduledObjectUpdate::Unchanged;
        }
        let Some(beam) = self.laser_bore_beam(object, powered) else {
            self.objects.schedule(id, u64::MAX);
            return ScheduledObjectUpdate::Unchanged;
        };
        let next_tick = self.simulation_tick.saturating_add(1);
        let Some(target) = beam.target else {
            if let Some(bore) = self.objects.object_mut(id) {
                bore.machine_target_y = u32::MAX;
                bore.growth_stage = 0;
            }
            self.objects.schedule(id, next_tick);
            return ScheduledObjectUpdate::Unchanged;
        };

        let damage = if object.machine_target_y == target.y {
            object.growth_stage.saturating_add(1)
        } else {
            1
        };
        if damage < LASER_BORE_TICKS_PER_TILE {
            if let Some(bore) = self.objects.object_mut(id) {
                bore.machine_target_y = target.y;
                bore.growth_stage = damage;
            }
            self.objects.schedule(id, next_tick);
            return ScheduledObjectUpdate::Unchanged;
        }

        let target_tile = self.tile_in_bounds(target.x, target.y, Layer::Foreground);
        let drop = mined_block_drop(target_tile);
        let can_store = drop.is_some_and(|(item, max_stack)| {
            self.objects
                .containers
                .get(&id)
                .is_some_and(|container| container.can_add(item, 1, max_stack))
        });
        let destroyed = can_store
            && self
                .set_tile(target.x, target.y, Layer::Foreground, TileId::EMPTY)
                .is_ok();
        if destroyed {
            let (item, max_stack) = drop.expect("storable laser-bore targets have a drop");
            let stored = self
                .objects
                .containers
                .get_mut(&id)
                .is_some_and(|container| container.try_add(item, 1, max_stack));
            debug_assert!(stored, "capacity was reserved before mining the tile");
        }
        if let Some(bore) = self.objects.object_mut(id) {
            bore.machine_target_y = if destroyed { u32::MAX } else { target.y };
            bore.growth_stage = if destroyed {
                0
            } else {
                LASER_BORE_TICKS_PER_TILE
            };
        }
        self.objects.schedule(id, next_tick);
        if destroyed {
            ScheduledObjectUpdate::TileDestroyed(target)
        } else {
            ScheduledObjectUpdate::Unchanged
        }
    }
}

pub(super) fn make_generated_object(
    id: ObjectId,
    object_type: ObjectTypeId,
    anchor: TilePos,
    root: TilePos,
    variant: u8,
    growth_stage: u8,
    next_update_tick: u64,
) -> WorldObject {
    WorldObject {
        id,
        object_type,
        anchor,
        root,
        width: 1,
        height: 1,
        variant,
        growth_stage,
        active: true,
        stored_energy_milli: 0,
        machine_target_y: u32::MAX,
        kill_count: 0,
        linked_object: 0,
        motion_position_milli: 0,
        next_update_tick,
    }
}

pub(super) fn populate_natural_objects(world: &mut World) {
    if world.width == 0 || world.height < 2 {
        return;
    }
    for y in 0..world.height {
        for x in 0..world.width {
            if world.tile_in_bounds(x, y, Layer::Foreground) != TileId::EMPTY {
                continue;
            }
            let anchor = TilePos { x, y };
            let hash = natural_hash(world.seed, x, y);
            let floor =
                (y + 1 < world.height).then(|| world.tile_in_bounds(x, y + 1, Layer::Foreground));
            let placement = if y > 0
                && world.tile_in_bounds(x, y - 1, Layer::Foreground) == super::ForegroundTile::GRASS
            {
                Some((NaturalObject::VINE, TilePos { x, y: y - 1 }))
            } else if floor == Some(super::ForegroundTile::GRASS)
                && hash.is_multiple_of(GRASS_SCATTER_DIVISOR)
            {
                Some((NaturalObject::GRASS, TilePos { x, y: y + 1 }))
            } else if floor.is_some_and(|tile| tile != TileId::EMPTY) {
                let divisor = if floor == Some(super::ForegroundTile::GRASS) {
                    GRASS_PEBBLE_SCATTER_DIVISOR
                } else {
                    PEBBLE_SCATTER_DIVISOR
                };
                (hash >> 12)
                    .is_multiple_of(divisor)
                    .then_some((NaturalObject::PEBBLE, TilePos { x, y: y + 1 }))
            } else {
                None
            };
            let Some((object_type, root)) = placement else {
                continue;
            };
            let id = world.objects.allocate_id();
            let (growth_stage, next_tick) = match object_type {
                NaturalObject::GRASS => (0, 8 + (hash >> 16) % 24),
                NaturalObject::VINE => (0, 3 + (hash >> 16) % 8),
                _ => (0, u64::MAX),
            };
            let object = make_generated_object(
                id,
                object_type,
                anchor,
                root,
                (hash >> 32) as u8,
                growth_stage,
                next_tick,
            );
            let _ = world.objects.insert(object);
        }
    }
}

fn growth_delay(seed: u64, id: ObjectId, tick: u64, minimum: u64, maximum: u64) -> u64 {
    let mut value = seed ^ id.raw().wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ tick.rotate_left(17);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    minimum + value % (maximum - minimum + 1)
}

fn natural_hash(seed: u64, x: u32, y: u32) -> u64 {
    let position = (u64::from(y) << 32) | u64::from(x);
    let mut value = seed ^ position.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn nature_hash(seed: u64, tick: u64, x: u32, y: u32, salt: u64) -> u64 {
    let position = (u64::from(y) << 32) | u64::from(x);
    let mut value = seed
        ^ tick.wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ position.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xA076_1D64_78BD_642F);
    value ^= value >> 32;
    value = value.wrapping_mul(0xE703_7ED1_A0B4_28DB);
    value ^ (value >> 29)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForegroundTile, ItemId, ItemStack, ObjectPlacementError};

    fn supported_world() -> World {
        let mut world = World::empty(128, 128, 7).unwrap();
        world
            .set_tile(10, 11, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        world
            .set_tile(20, 9, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        world
    }

    fn power_laser_bore(world: &mut World) -> PowerSystem {
        for x in [9, 11, 12] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(9, 6))
            .unwrap();
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(11, 5))
            .unwrap();
        let mut power = PowerSystem::new();
        power.distribute(world, 0.5, Duration::from_secs(1));
        power
    }

    #[test]
    fn breaking_a_root_removes_its_objects() {
        let mut world = supported_world();
        world
            .place_natural_object(
                NaturalObject::GRASS,
                TilePos::new(10, 10),
                TilePos::new(10, 11),
            )
            .unwrap();
        assert_eq!(world.object_count(), 1);
        world
            .set_tile(10, 11, Layer::Foreground, TileId::EMPTY)
            .unwrap();
        assert_eq!(world.object_count(), 0);
    }

    #[test]
    fn vine_grows_without_scanning_unrelated_objects() {
        let mut world = supported_world();
        let vine = world
            .place_natural_object(
                NaturalObject::VINE,
                TilePos::new(20, 10),
                TilePos::new(20, 9),
            )
            .unwrap();
        let update = world.update_decorations(Duration::from_secs(4), 8);
        assert_eq!(update.objects_processed, 1);
        assert_eq!(update.objects_grown, 1);
        assert_eq!(world.object(vine).unwrap().size(), [1, 2]);
    }

    #[test]
    fn occupied_cells_prevent_overlapping_objects() {
        let mut world = supported_world();
        world
            .place_natural_object(
                NaturalObject::GRASS,
                TilePos::new(10, 10),
                TilePos::new(10, 11),
            )
            .unwrap();
        assert!(matches!(
            world.place_natural_object(
                NaturalObject::PEBBLE,
                TilePos::new(10, 10),
                TilePos::new(10, 11),
            ),
            Err(ObjectPlacementError::Occupied(_))
        ));
    }

    #[test]
    fn nature_ticks_spawn_valid_plants_and_spread_exposed_grass() {
        let mut world = World::empty(8, 8, 19).unwrap();
        world
            .set_tile(2, 4, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        world
            .set_tile(4, 2, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        world
            .set_tile(5, 4, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        world
            .set_tile(6, 4, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
        let config = NatureSimulationConfig {
            horizontal_radius_tiles: 8,
            vertical_radius_tiles: 8,
            columns_per_tick: 8,
            max_columns_per_update: 8,
            object_update_budget: 8,
            grass_spawn_chance: 1,
            vine_spawn_chance: 1,
            grass_spread_chance: 1,
        };

        let early = world.update_nature(Duration::from_millis(999), TilePos::new(4, 4), config);
        assert_eq!(early.columns_scanned, 0);
        let update = world.update_nature(Duration::from_millis(1), TilePos::new(4, 4), config);

        assert!(update.grass_spawned >= 1);
        assert!(update.vines_spawned >= 1);
        assert_eq!(update.grass_tiles_spread, 1);
        assert_eq!(update.changed_tiles(), &[TilePos::new(6, 4)]);
        assert_eq!(
            world.tile(6, 4, Layer::Foreground).unwrap(),
            ForegroundTile::GRASS
        );
        assert!(world.objects().any(|object| {
            object.object_type() == NaturalObject::GRASS && object.anchor() == TilePos::new(2, 3)
        }));
        assert!(world.objects().any(|object| {
            object.object_type() == NaturalObject::VINE && object.anchor() == TilePos::new(4, 3)
        }));
    }

    #[test]
    fn nature_work_is_capped_after_a_long_frame() {
        let mut world = World::empty(128, 64, 3).unwrap();
        let config = NatureSimulationConfig {
            horizontal_radius_tiles: 128,
            vertical_radius_tiles: 64,
            columns_per_tick: 16,
            max_columns_per_update: 3,
            ..NatureSimulationConfig::default()
        };
        let update = world.update_nature(Duration::from_secs(30), TilePos::new(64, 32), config);
        assert_eq!(update.columns_scanned, 3);
    }

    #[test]
    fn laser_bore_destroys_one_target_after_three_scheduled_ticks() {
        let mut world = World::empty(16, 80, 0).unwrap();
        for x in [4, 6] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
            .unwrap();
        assert!(world.set_furniture_active(bore, true));
        let power = power_laser_bore(&mut world);

        let beam = world
            .laser_bore_beam(world.object(bore).unwrap(), power.is_powered(bore))
            .unwrap();
        assert_eq!(beam.length_tiles, 4);
        assert_eq!(beam.target, Some(TilePos::new(5, 12)));

        for _ in 0..2 {
            let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
            assert_eq!(update.tiles_destroyed, 0);
            assert_eq!(
                world.tile(5, 12, Layer::Foreground).unwrap(),
                ForegroundTile::STONE
            );
        }
        let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        assert_eq!(update.tiles_destroyed, 1);
        assert_eq!(update.changed_tiles(), &[TilePos::new(5, 12)]);
        assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
        assert_eq!(
            world.container(bore).unwrap().slot(0),
            ItemStack::new(ItemId::STONE_BLOCK, 1)
        );
        assert!(world.object(bore).is_some());
    }

    #[test]
    fn laser_bore_stays_idle_until_activated_and_stops_when_deactivated() {
        let mut world = World::empty(16, 80, 0).unwrap();
        for x in [4, 6] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
            .unwrap();
        let power = power_laser_bore(&mut world);

        assert!(!world.object(bore).unwrap().is_active());
        assert!(
            world
                .laser_bore_beam(world.object(bore).unwrap(), power.is_powered(bore))
                .is_none()
        );
        let idle = world.update_decorations(Duration::from_secs(10), 8);
        assert_eq!(idle.objects_processed, 0);

        assert!(world.set_furniture_active(bore, true));
        world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        assert!(world.set_furniture_active(bore, false));
        let stopped = world.update_decorations(Duration::from_secs(10), 8);
        assert_eq!(stopped.tiles_destroyed, 0);
        assert_eq!(
            world.tile(5, 12, Layer::Foreground).unwrap(),
            ForegroundTile::STONE
        );
    }

    #[test]
    fn active_laser_bore_pauses_until_its_grid_is_energized() {
        let mut world = World::empty(16, 80, 0).unwrap();
        for x in [4, 6] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
            .unwrap();
        assert!(world.set_furniture_active(bore, true));
        let mut power = PowerSystem::new();
        power.update(&world);

        for _ in 0..4 {
            world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        }
        assert_eq!(
            world.tile(5, 12, Layer::Foreground).unwrap(),
            ForegroundTile::STONE
        );
        assert_eq!(world.object(bore).unwrap().growth_stage(), 0);

        power = power_laser_bore(&mut world);
        assert!(power.is_powered(bore));
        for _ in 0..3 {
            world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        }
        assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
    }

    #[test]
    fn laser_bore_pauses_without_destroying_a_block_when_storage_is_full() {
        let mut world = World::empty(16, 80, 0).unwrap();
        for x in [4, 6] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
            .unwrap();
        assert!(world.set_furniture_active(bore, true));
        let power = power_laser_bore(&mut world);
        for slot in 0..usize::from(crate::LASER_BORE_SLOTS) {
            assert!(
                world
                    .container_mut(bore)
                    .unwrap()
                    .set_slot(slot, ItemStack::new(ItemId::DIRT_BLOCK, 999),)
            );
        }

        for _ in 0..4 {
            world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        }
        assert_eq!(
            world.tile(5, 12, Layer::Foreground).unwrap(),
            ForegroundTile::STONE
        );

        assert!(world.container_mut(bore).unwrap().set_slot(0, None));
        let update = world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        assert_eq!(update.tiles_destroyed, 1);
        assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
        assert_eq!(
            world.container(bore).unwrap().slot(0),
            ItemStack::new(ItemId::STONE_BLOCK, 1)
        );
    }

    #[test]
    fn laser_bore_mines_outside_the_players_active_area() {
        let mut world = World::empty(512, 80, 0).unwrap();
        for x in [4, 6] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
            .unwrap();
        assert!(world.set_furniture_active(bore, true));
        let power = power_laser_bore(&mut world);
        let config = NatureSimulationConfig {
            horizontal_radius_tiles: 0,
            vertical_radius_tiles: 0,
            columns_per_tick: 1,
            max_columns_per_update: 1,
            object_update_budget: 1,
            ..NatureSimulationConfig::default()
        };
        let player = TilePos::new(500, 70);

        for _ in 0..2 {
            let update =
                world.update_nature_with_power(Duration::from_secs(1), player, config, &power);
            assert_eq!(update.decorations.objects_processed, 1);
            assert_eq!(update.decorations.tiles_destroyed, 0);
            assert_eq!(update.columns_scanned, 1);
        }
        let update = world.update_nature_with_power(Duration::from_secs(1), player, config, &power);
        assert_eq!(update.decorations.objects_processed, 1);
        assert_eq!(update.decorations.tiles_destroyed, 1);
        assert_eq!(update.changed_tiles(), &[TilePos::new(5, 12)]);
        assert_eq!(world.tile(5, 12, Layer::Foreground).unwrap(), TileId::EMPTY);
    }

    #[test]
    fn laser_bore_never_scans_beyond_four_hundred_tiles() {
        let mut world = World::empty(8, 500, 0).unwrap();
        for x in [2, 4] {
            world
                .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(3, 406, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 3))
            .unwrap();
        assert!(world.set_furniture_active(bore, true));

        let beam = world
            .laser_bore_beam(world.object(bore).unwrap(), true)
            .unwrap();
        assert_eq!(beam.first_y, 6);
        assert_eq!(beam.length_tiles, 400);
        assert_eq!(beam.target, None);
        for _ in 0..4 {
            world.update_decorations(Duration::from_secs(1), 8);
        }
        assert_eq!(
            world.tile(3, 406, Layer::Foreground).unwrap(),
            ForegroundTile::STONE
        );
    }

    #[test]
    fn laser_bore_tracks_and_mines_targets_beyond_u8_range() {
        let mut world = World::empty(16, 450, 0).unwrap();
        for x in [2, 4, 6, 8, 9] {
            world
                .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(3, 306, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 3))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 4))
            .unwrap();
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(8, 3))
            .unwrap();
        assert!(world.set_furniture_active(bore, true));
        let mut power = PowerSystem::new();
        power.distribute(&mut world, 0.5, Duration::from_secs(1));

        for _ in 0..3 {
            world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
        }
        assert_eq!(
            world.tile(3, 306, Layer::Foreground).unwrap(),
            TileId::EMPTY
        );
    }

    #[test]
    fn dirt_becoming_grass_keeps_generic_surface_decorations() {
        let mut world = World::empty(8, 8, 0).unwrap();
        world
            .set_tile(2, 3, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
        let pebble = world
            .place_natural_object(
                NaturalObject::PEBBLE,
                TilePos::new(2, 2),
                TilePos::new(2, 3),
            )
            .unwrap();
        world
            .set_tile(2, 3, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        assert!(world.object(pebble).is_some());
    }

    #[test]
    fn decoration_revision_tracks_placement_and_growth() {
        let mut world = supported_world();
        let initial = world.object_revision();
        world
            .place_natural_object(
                NaturalObject::VINE,
                TilePos::new(20, 10),
                TilePos::new(20, 9),
            )
            .unwrap();
        let placed = world.object_revision();
        assert_ne!(placed, initial);
        world.update_decorations(Duration::from_secs(4), 8);
        assert_ne!(world.object_revision(), placed);
    }
}
