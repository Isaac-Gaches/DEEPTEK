use super::objects::{
    DecorationUpdate, ObjectId, ObjectTypeId, RemovedObject, TilePos, WorldObject,
};
// Budgeted natural growth and automated terrain interactions.

#[cfg(test)]
use super::FurnitureObject;
use super::{
    BoreBeamVisual, FurnitureBehavior, LASER_BORE_TICKS_PER_TILE, Layer, NaturalObject, TileId,
    World, furniture_definition,
};
use crate::PowerSystem;
use crate::items::mined_block_drop;
use std::time::Duration;

const SIMULATION_TICK_NANOS: u64 = 1_000_000_000;
const GRASS_SCATTER_DIVISOR: u64 = 4;
const GRASS_PEBBLE_SCATTER_DIVISOR: u64 = 16;
const PEBBLE_SCATTER_DIVISOR: u64 = 64;
const HANGING_STONE_SCATTER_DIVISOR: u64 = 32;
pub const MAX_VINE_LENGTH: u16 = 10;

enum ScheduledObjectUpdate {
    Unchanged,
    Grown,
    TilesDestroyed(Vec<super::BrokenTile>),
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
            horizontal_radius_tiles: 384,
            vertical_radius_tiles: 256,
            columns_per_tick: 8,
            max_columns_per_update: 32,
            object_update_budget: 512,
            grass_spawn_chance: 6,
            vine_spawn_chance: 4,
            grass_spread_chance: 8,
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

    pub fn detached_objects(&self) -> &[RemovedObject] {
        self.decorations.detached_objects()
    }

    pub fn broken_tiles(&self) -> &[super::BrokenTile] {
        self.decorations.broken_tiles()
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
                ScheduledObjectUpdate::TilesDestroyed(tiles) => {
                    update.tiles_destroyed += tiles.len();
                    for tile in tiles {
                        update.changed_tiles.push(tile.position);
                        update
                            .detached_objects
                            .extend(tile.unsupported_objects.iter().cloned());
                        update.broken_tiles.push(tile);
                    }
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
                self.objects.schedule(id, u64::MAX);
                ScheduledObjectUpdate::Unchanged
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
                let delay = growth_delay(self.seed, id, self.simulation_tick, 6, 16);
                self.objects
                    .schedule(id, self.simulation_tick.saturating_add(delay));
                if grew {
                    ScheduledObjectUpdate::Grown
                } else {
                    ScheduledObjectUpdate::Unchanged
                }
            }
            _ => match furniture_definition(object.object_type).map(|value| value.behavior()) {
                Some(FurnitureBehavior::VerticalBore(behavior)) => match behavior.beam {
                    BoreBeamVisual::Cyan => self.update_laser_bore(
                        id,
                        &object,
                        power.is_some_and(|power| power.is_powered(id)),
                    ),
                    BoreBeamVisual::Red => self.update_red_shaft_bore(
                        id,
                        &object,
                        power.is_some_and(|power| power.is_powered(id)),
                    ),
                },
                Some(FurnitureBehavior::DirectionalDrill) => self.update_laser_drill(
                    id,
                    &object,
                    power.is_some_and(|power| power.is_powered(id)),
                ),
                _ => {
                    self.objects.schedule(id, u64::MAX);
                    ScheduledObjectUpdate::Unchanged
                }
            },
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
        let Some(target_y) = beam.target_y else {
            if let Some(bore) = self.objects.object_mut(id) {
                bore.machine_target_y = u32::MAX;
                bore.growth_stage = 0;
            }
            self.objects.schedule(id, next_tick);
            return ScheduledObjectUpdate::Unchanged;
        };
        let targets: Vec<_> = (beam.first_x..beam.first_x + beam.width)
            .filter_map(|x| {
                let tile = self.tile_in_bounds(x, target_y, Layer::Foreground);
                mined_block_drop(tile, Layer::Foreground)
                    .map(|drop| (TilePos::new(x, target_y), drop))
            })
            .collect();
        let can_store_all =
            self.objects
                .containers
                .get(&id)
                .cloned()
                .is_some_and(|mut container| {
                    targets
                        .iter()
                        .all(|&(_, (item, max_stack))| container.try_add(item, 1, max_stack))
                });
        let mut destroyed = Vec::with_capacity(beam.width as usize);
        let drill_speed_percent = self.specialist_bonuses().drill_speed_percent();
        for &(target, (item, max_stack)) in &targets {
            let Some(broken) = apply_drill_damage(self, target, can_store_all, drill_speed_percent)
            else {
                continue;
            };
            if can_store_all {
                let stored = self
                    .objects
                    .containers
                    .get_mut(&id)
                    .is_some_and(|container| container.try_add(item, 1, max_stack));
                debug_assert!(stored, "laser-bore storage was reserved before mining");
                destroyed.push(broken);
            }
        }
        if let Some(bore) = self.objects.object_mut(id) {
            bore.machine_target_y = if destroyed.is_empty() {
                target_y
            } else {
                u32::MAX
            };
            bore.growth_stage = if destroyed.is_empty() {
                LASER_BORE_TICKS_PER_TILE
            } else {
                0
            };
        }
        self.objects.schedule(id, next_tick);
        if destroyed.is_empty() {
            ScheduledObjectUpdate::Unchanged
        } else {
            ScheduledObjectUpdate::TilesDestroyed(destroyed)
        }
    }

    fn update_laser_drill(
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
        let Some(beam) = self.laser_drill_beam(object, powered) else {
            self.objects.schedule(id, u64::MAX);
            return ScheduledObjectUpdate::Unchanged;
        };
        self.update_single_tile_drill(id, beam.target)
    }

    fn update_single_tile_drill(
        &mut self,
        id: ObjectId,
        target: Option<TilePos>,
    ) -> ScheduledObjectUpdate {
        let next_tick = self.simulation_tick.saturating_add(1);
        let Some(target) = target else {
            if let Some(bore) = self.objects.object_mut(id) {
                bore.machine_target_y = u32::MAX;
                bore.growth_stage = 0;
            }
            self.objects.schedule(id, next_tick);
            return ScheduledObjectUpdate::Unchanged;
        };

        let target_tile = self.tile_in_bounds(target.x, target.y, Layer::Foreground);
        let drop = mined_block_drop(target_tile, Layer::Foreground);
        let can_store = drop.is_some_and(|(item, max_stack)| {
            self.objects
                .containers
                .get(&id)
                .is_some_and(|container| container.can_add(item, 1, max_stack))
        });
        let drill_speed_percent = self.specialist_bonuses().drill_speed_percent();
        let broken = apply_drill_damage(self, target, can_store, drill_speed_percent);
        if broken.is_some() {
            let (item, max_stack) = drop.expect("storable laser-bore targets have a drop");
            let stored = self
                .objects
                .containers
                .get_mut(&id)
                .is_some_and(|container| container.try_add(item, 1, max_stack));
            debug_assert!(stored, "capacity was reserved before mining the tile");
        }
        if let Some(bore) = self.objects.object_mut(id) {
            bore.machine_target_y = if broken.is_some() { u32::MAX } else { target.y };
            bore.growth_stage = 0;
        }
        self.objects.schedule(id, next_tick);
        broken.map_or(ScheduledObjectUpdate::Unchanged, |broken| {
            ScheduledObjectUpdate::TilesDestroyed(vec![broken])
        })
    }

    fn update_red_shaft_bore(
        &mut self,
        id: ObjectId,
        object: &WorldObject,
        powered: bool,
    ) -> ScheduledObjectUpdate {
        if !object.active {
            self.objects.schedule(id, u64::MAX);
            return ScheduledObjectUpdate::Unchanged;
        }
        let next_tick = self.simulation_tick.saturating_add(1);
        if !powered {
            self.objects.schedule(id, next_tick);
            return ScheduledObjectUpdate::Unchanged;
        }
        let Some(beam) = self.red_shaft_bore_beam(object, powered) else {
            self.objects.schedule(id, u64::MAX);
            return ScheduledObjectUpdate::Unchanged;
        };
        let Some(target_y) = beam.target_y else {
            if let Some(bore) = self.objects.object_mut(id) {
                bore.machine_target_y = u32::MAX;
                bore.growth_stage = 0;
            }
            self.objects.schedule(id, next_tick);
            return ScheduledObjectUpdate::Unchanged;
        };
        let targets: Vec<_> = (beam.first_x..beam.first_x + beam.width)
            .filter_map(|x| {
                let tile = self.tile_in_bounds(x, target_y, Layer::Foreground);
                mined_block_drop(tile, Layer::Foreground)
                    .map(|drop| (TilePos::new(x, target_y), drop))
            })
            .collect();
        let can_store_all =
            self.objects
                .containers
                .get(&id)
                .cloned()
                .is_some_and(|mut container| {
                    targets
                        .iter()
                        .all(|&(_, (item, max_stack))| container.try_add(item, 1, max_stack))
                });
        let mut destroyed = Vec::with_capacity(beam.width as usize);
        let drill_speed_percent = self.specialist_bonuses().drill_speed_percent();
        for &(target, (item, max_stack)) in &targets {
            let Some(broken) = apply_drill_damage(self, target, can_store_all, drill_speed_percent)
            else {
                continue;
            };
            if can_store_all {
                let stored = self
                    .objects
                    .containers
                    .get_mut(&id)
                    .is_some_and(|container| container.try_add(item, 1, max_stack));
                debug_assert!(stored, "shaft-bore storage was reserved before mining");
                destroyed.push(broken);
            }
        }
        if let Some(bore) = self.objects.object_mut(id) {
            bore.machine_target_y = if destroyed.is_empty() {
                target_y
            } else {
                u32::MAX
            };
            bore.growth_stage = if destroyed.is_empty() {
                LASER_BORE_TICKS_PER_TILE
            } else {
                0
            };
        }
        self.objects.schedule(id, next_tick);
        if destroyed.is_empty() {
            ScheduledObjectUpdate::Unchanged
        } else {
            ScheduledObjectUpdate::TilesDestroyed(destroyed)
        }
    }
}

fn apply_drill_damage(
    world: &mut World,
    target: TilePos,
    may_break: bool,
    speed_percent: u16,
) -> Option<super::BrokenTile> {
    let health = world
        .block_health(target, Layer::Foreground)
        .ok()
        .flatten()?;
    let mut damage = health
        .maximum()
        .div_ceil(u16::from(LASER_BORE_TICKS_PER_TILE.max(1)));
    damage = ((u32::from(damage) * u32::from(speed_percent.max(1))).div_ceil(100))
        .min(u32::from(u16::MAX)) as u16;
    if !may_break {
        damage = damage.min(health.current().saturating_sub(1));
    }
    (damage > 0)
        .then(|| world.damage_block(target, Layer::Foreground, damage).ok())
        .flatten()
        .and_then(|result| result.broken)
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
        health: 0,
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
            } else if y > 0
                && world.tile_in_bounds(x, y - 1, Layer::Foreground) == super::ForegroundTile::STONE
                && (hash >> 20).is_multiple_of(HANGING_STONE_SCATTER_DIVISOR)
            {
                Some((NaturalObject::HANGING_STONE, TilePos { x, y: y - 1 }))
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
                NaturalObject::GRASS => (0, u64::MAX),
                NaturalObject::VINE => (0, 6 + (hash >> 16) % 11),
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

pub(super) fn natural_hash(seed: u64, x: u32, y: u32) -> u64 {
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
mod tests;
