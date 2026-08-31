mod biomes;
mod blocks;
mod decorations;
mod durability;
mod furniture;
mod generator;
mod nature;
mod objects;
mod persistence;
mod player_state;
mod structures;
mod survey;
mod world_objects;

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::io;
use std::path::Path;

pub use biomes::{BiomeId, BiomeMap};
pub use blocks::{BUILT_IN_BLOCKS, BlockDefinition, background_tile_for, block_definition};
pub use decorations::{
    BUILT_IN_DECORATIONS, DecorationDefinition, DecorationVisual, NaturalObject,
    POWERED_CABLE_OBJECT, ROPE_OBJECT, decoration_definition,
};
pub use durability::{BlockDamage, BlockHealth, DEFAULT_BLOCK_HEALTH};
pub use furniture::{
    AMMO_TURRET_DEFINITION, AMMO_TURRET_DEMAND_MILLI_PER_SECOND, AMMO_TURRET_SLOTS,
    BATTERY_CAPACITY_MILLI, BATTERY_DEFINITION, BED_DEFINITION, BUILT_IN_FURNITURE,
    CARGO_CONVEYOR_DEFINITION, CARGO_LIFT_DEFINITION, CARGO_LIFT_DEMAND_MILLI_PER_SECOND,
    CARGO_LIFT_SLOTS, CARGO_LIFT_SPEED_MILLI_TILES_PER_SECOND, CHEST_DEFINITION,
    COMPOSITE_ASSEMBLER_DEFINITION, COMPOSITE_ASSEMBLER_DEMAND_MILLI_PER_SECOND,
    COMPOSITE_ASSEMBLER_SLOTS, CargoLiftDirection, ChunkActivity, DEFAULT_MACHINE_HEALTH,
    DIRECTIONAL_SENTRY_DEFINITION, DIRECTIONAL_SENTRY_DEMAND_MILLI_PER_SECOND, DOOR_DEFINITION,
    FurnitureConfiguration, FurnitureDefinition, FurnitureFacing, FurnitureInteraction,
    FurnitureObject, FurnitureSupport, ItemTransportRole, LASER_BORE_DEFINITION,
    LASER_BORE_DEMAND_MILLI_PER_SECOND, LASER_BORE_MAX_LENGTH, LASER_BORE_SLOTS,
    LASER_BORE_TICKS_PER_TILE, LASER_DRILL_DEFINITION, LASER_DRILL_DEMAND_MILLI_PER_SECOND,
    LASER_DRILL_MAX_LENGTH, LASER_DRILL_SLOTS, LIFT_STATION_DEFINITION, LIFT_STATION_SLOTS,
    LaserDrillAim, LiftStationConfiguration, LiftStationMode,
    ORBITAL_EXPORT_DEMAND_MILLI_PER_SECOND, ORBITAL_EXPORT_LAUNCHER_DEFINITION,
    ORBITAL_EXPORT_LAUNCHER_SLOTS, POWER_CONNECTION_RANGE_HALF_TILES, POWER_CONNECTION_RANGE_TILES,
    POWER_CONNECTOR_DEFINITION, POWER_CONNECTOR_RANGE_TILES, POWERED_CABLE_ANCHOR_DEFINITION,
    PROCUREMENT_TERMINAL_DEFINITION, PROCUREMENT_TERMINAL_DEMAND_MILLI_PER_SECOND,
    PYLON_DEFINITION, PowerRole, RED_SHAFT_BORE_DEFINITION, RED_SHAFT_BORE_DEMAND_MILLI_PER_SECOND,
    RED_SHAFT_BORE_SLOTS, RED_SHAFT_BORE_WIDTH, SOLAR_ARRAY_DEFINITION,
    SOLAR_GENERATION_MILLI_PER_SECOND, SPIKES_DEFINITION, SUBSURFACE_SURVEY_DEPTH,
    SUBSURFACE_SURVEY_WIDTH, SUBSURFACE_SURVEYOR_DEFINITION,
    SUBSURFACE_SURVEYOR_DEMAND_MILLI_PER_SECOND, TURRET_DEFINITION, TURRET_DEMAND_MILLI_PER_SECOND,
    TargetPriority, furniture_definition,
};
pub(crate) use furniture::{
    LaserBoreBeam, LaserDrillBeam, RedShaftBoreBeam, configuration_variant,
};
pub use generator::WorldGenerator;
pub use nature::{MAX_VINE_LENGTH, NatureSimulationConfig, NatureUpdate};
pub use objects::{
    DecorationUpdate, MachineDamage, MachineHealth, ObjectId, ObjectPlacementError, ObjectTypeId,
    RemovedObject, TilePos, WorldObject,
};
pub use player_state::PlayerState;
pub use survey::{MAX_SURVEY_ORE_TYPES, OreEstimate, SubsurfaceSurvey};

pub const CHUNK_SIZE: usize = 64;
const CHUNK_AREA: usize = CHUNK_SIZE * CHUNK_SIZE;
pub const MAX_WORLD_WIDTH: u32 = 10_000;
pub const MAX_WORLD_HEIGHT: u32 = 16_000;
pub const MAX_WORLD_TILES: u64 = 64_000_000;
pub const MAX_WORLD_NAME_BYTES: usize = 64;
/// Surface datum. At the maximum 16,000-tile height this leaves roughly
/// 14,000 tiles of underground terrain beneath the player.
pub const SEA_LEVEL_PERCENT: u32 = 12;
pub const METRES_PER_TILE: f32 = 0.7;
/// Solar arrays and orbital exporters shut down at or below this elevation.
pub const SKY_MACHINE_MIN_ELEVATION_DECIMETRES: i32 = -1_000;
/// Late morning, expressed as a normalized fraction of the day cycle.
pub const DEFAULT_TIME_OF_DAY: f32 = 0.42;
const FOREGROUND_CHANGE_HISTORY: usize = 4_096;

/// Runtime-only bounded terrain journal used by sparse systems whose topology
/// depends on foreground line of sight. It is deliberately ignored by world
/// equality and persistence because it is derived invalidation state.
#[derive(Clone, Debug, Default)]
struct ForegroundChangeLog {
    revision: u64,
    positions: VecDeque<TilePos>,
}

/// Runtime-only invalidation counter for room geometry. It intentionally does
/// not participate in save equality because cached room data is reconstructed.
#[derive(Clone, Copy, Debug, Default)]
struct HousingChangeRevision(u64);

impl PartialEq for HousingChangeRevision {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for HousingChangeRevision {}

impl PartialEq for ForegroundChangeLog {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for ForegroundChangeLog {}

impl ForegroundChangeLog {
    fn record(&mut self, position: TilePos) {
        self.revision = self.revision.saturating_add(1);
        if self.positions.len() == FOREGROUND_CHANGE_HISTORY {
            self.positions.pop_front();
        }
        self.positions.push_back(position);
    }

    fn since(&self, revision: u64) -> Option<impl Iterator<Item = TilePos> + '_> {
        if revision > self.revision {
            return None;
        }
        let retained = self.positions.len() as u64;
        let oldest = self.revision.saturating_sub(retained).saturating_add(1);
        if revision < oldest.saturating_sub(1) {
            return None;
        }
        let skip = revision.saturating_add(1).saturating_sub(oldest) as usize;
        Some(self.positions.iter().skip(skip).copied())
    }
}

/// A stable, serialized tile identifier. Zero is reserved for empty space.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct TileId(u16);

impl TileId {
    pub const EMPTY: Self = Self(0);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

pub struct ForegroundTile;

impl ForegroundTile {
    pub const AIR: TileId = TileId::EMPTY;
    pub const GRASS: TileId = TileId::new(1);
    pub const DIRT: TileId = TileId::new(2);
    pub const STONE: TileId = TileId::new(3);
    pub const IRON_ORE: TileId = TileId::new(5);
    pub const ASTERITE: TileId = TileId::new(7);
}

pub struct BackgroundTile;

impl BackgroundTile {
    pub const NONE: TileId = TileId::EMPTY;
    pub const DIRT_WALL: TileId = TileId::new(1);
    pub const STONE_WALL: TileId = TileId::new(2);
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Layer {
    Foreground,
    Background,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrokenTile {
    pub position: TilePos,
    pub layer: Layer,
    pub tile: TileId,
    pub unsupported_objects: Vec<RemovedObject>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChunkPos {
    pub x: u32,
    pub y: u32,
}

/// Two cache-friendly tile planes. Edge chunks retain unused empty cells so every
/// chunk has the same memory layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    foreground: Box<[TileId]>,
    background: Box<[TileId]>,
}

impl Chunk {
    fn empty() -> Self {
        Self {
            foreground: vec![TileId::EMPTY; CHUNK_AREA].into_boxed_slice(),
            background: vec![TileId::EMPTY; CHUNK_AREA].into_boxed_slice(),
        }
    }

    pub fn tiles(&self, layer: Layer) -> &[TileId] {
        match layer {
            Layer::Foreground => &self.foreground,
            Layer::Background => &self.background,
        }
    }

    pub fn tiles_mut(&mut self, layer: Layer) -> &mut [TileId] {
        match layer {
            Layer::Foreground => &mut self.foreground,
            Layer::Background => &mut self.background,
        }
    }

    fn tile(&self, local_x: usize, local_y: usize, layer: Layer) -> TileId {
        self.tiles(layer)[local_y * CHUNK_SIZE + local_x]
    }

    fn set_tile(&mut self, local_x: usize, local_y: usize, layer: Layer, tile: TileId) {
        self.tiles_mut(layer)[local_y * CHUNK_SIZE + local_x] = tile;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    name: String,
    width: u32,
    height: u32,
    seed: u64,
    chunks_wide: u32,
    chunks_high: u32,
    chunks: Vec<Chunk>,
    biomes: BiomeMap,
    objects: objects::ObjectStore,
    simulation_tick: u64,
    simulation_remainder_nanos: u64,
    player_position_bits: Option<[u32; 2]>,
    player_state: Option<PlayerState>,
    time_of_day_bits: u32,
    foreground_changes: ForegroundChangeLog,
    housing_changes: HousingChangeRevision,
    block_damage: durability::BlockDamageStore,
    pub(crate) specialists: Vec<crate::SpecialistRecord>,
}

impl World {
    pub fn empty(width: u32, height: u32, seed: u64) -> Result<Self, WorldError> {
        validate_dimensions(width, height)?;
        let chunks_wide = width.div_ceil(CHUNK_SIZE as u32);
        let chunks_high = height.div_ceil(CHUNK_SIZE as u32);
        let count = (chunks_wide as usize)
            .checked_mul(chunks_high as usize)
            .ok_or(WorldError::InvalidDimensions { width, height })?;
        let chunks = (0..count).map(|_| Chunk::empty()).collect();
        Ok(Self {
            name: String::new(),
            width,
            height,
            seed,
            chunks_wide,
            chunks_high,
            chunks,
            biomes: BiomeMap::normal(chunks_wide, chunks_high),
            objects: objects::ObjectStore::new(count, chunks_wide),
            simulation_tick: 0,
            simulation_remainder_nanos: 0,
            player_position_bits: None,
            player_state: None,
            time_of_day_bits: DEFAULT_TIME_OF_DAY.to_bits(),
            foreground_changes: ForegroundChangeLog::default(),
            housing_changes: HousingChangeRevision::default(),
            block_damage: durability::BlockDamageStore::default(),
            specialists: Vec::new(),
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn sea_level_y(&self) -> u32 {
        self.height * SEA_LEVEL_PERCENT / 100
    }

    /// Signed elevation relative to sea level in decimetres. World Y grows
    /// downward, so positions above sea level are positive and those below it
    /// are negative.
    pub fn elevation_decimetres(&self, world_y: f32) -> i32 {
        if !world_y.is_finite() {
            return 0;
        }
        ((self.sea_level_y() as f32 - world_y) * (METRES_PER_TILE * 10.0)).round() as i32
    }

    pub const fn seed(&self) -> u64 {
        self.seed
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) -> Result<(), WorldError> {
        let name = name.into();
        if name.len() > MAX_WORLD_NAME_BYTES {
            return Err(WorldError::InvalidData(format!(
                "world name exceeds {MAX_WORLD_NAME_BYTES} UTF-8 bytes"
            )));
        }
        self.name = name;
        Ok(())
    }

    /// The last saved player centre in world-space coordinates. Older saves
    /// and newly generated worlds return `None` until a session is saved.
    pub fn player_position(&self) -> Option<[f32; 2]> {
        self.player_position_bits
            .map(|position| position.map(f32::from_bits))
    }

    pub fn set_player_position(&mut self, position: Option<[f32; 2]>) -> Result<(), WorldError> {
        if let Some([x, y]) = position
            && (!x.is_finite()
                || !y.is_finite()
                || !(0.0..self.width as f32).contains(&x)
                || !(0.0..self.height as f32).contains(&y))
        {
            return Err(WorldError::InvalidData(
                "player position is outside the world".into(),
            ));
        }
        self.player_position_bits = position.map(|position| position.map(f32::to_bits));
        Ok(())
    }

    pub const fn player_state(&self) -> Option<&PlayerState> {
        self.player_state.as_ref()
    }

    pub fn set_player_state(&mut self, state: Option<PlayerState>) {
        self.player_state = state;
    }

    pub fn time_of_day(&self) -> f32 {
        f32::from_bits(self.time_of_day_bits)
    }

    pub fn set_time_of_day(&mut self, time_of_day: f32) -> Result<(), WorldError> {
        if !time_of_day.is_finite() {
            return Err(WorldError::InvalidData("time of day must be finite".into()));
        }
        self.time_of_day_bits = time_of_day.rem_euclid(1.0).to_bits();
        Ok(())
    }

    /// Reads only header metadata; it does not decompress terrain or objects.
    pub fn read_name(path: impl AsRef<Path>) -> Result<Option<String>, WorldError> {
        persistence::read_name(path.as_ref())
    }

    pub const fn chunks_wide(&self) -> u32 {
        self.chunks_wide
    }

    pub const fn chunks_high(&self) -> u32 {
        self.chunks_high
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunk_index(pos).map(|index| &self.chunks[index])
    }

    pub fn chunk_mut(&mut self, pos: ChunkPos) -> Option<&mut Chunk> {
        self.chunk_index(pos).map(|index| &mut self.chunks[index])
    }

    pub fn chunks(&self) -> impl ExactSizeIterator<Item = (ChunkPos, &Chunk)> {
        let width = self.chunks_wide as usize;
        self.chunks.iter().enumerate().map(move |(index, chunk)| {
            (
                ChunkPos {
                    x: (index % width) as u32,
                    y: (index / width) as u32,
                },
                chunk,
            )
        })
    }

    pub fn tile(&self, x: u32, y: u32, layer: Layer) -> Result<TileId, WorldError> {
        let (chunk_index, local_x, local_y) = self.locate(x, y)?;
        Ok(self.chunks[chunk_index].tile(local_x, local_y, layer))
    }

    /// Fast crate-internal access for loops that have already validated coordinates.
    #[inline]
    pub(crate) fn tile_in_bounds(&self, x: u32, y: u32, layer: Layer) -> TileId {
        debug_assert!(x < self.width && y < self.height);
        let chunk_x = x / CHUNK_SIZE as u32;
        let chunk_y = y / CHUNK_SIZE as u32;
        let chunk_index = (chunk_y * self.chunks_wide + chunk_x) as usize;
        self.chunks[chunk_index].tile(x as usize % CHUNK_SIZE, y as usize % CHUNK_SIZE, layer)
    }

    pub fn set_tile(
        &mut self,
        x: u32,
        y: u32,
        layer: Layer,
        tile: TileId,
    ) -> Result<(), WorldError> {
        let (chunk_index, local_x, local_y) = self.locate(x, y)?;
        let previous = self.chunks[chunk_index].tile(local_x, local_y, layer);
        if previous == tile {
            return Ok(());
        }
        if layer == Layer::Foreground {
            let position = TilePos { x, y };
            if tile == TileId::EMPTY {
                if let Some(object) = self.objects.has_non_empty_container_rooted_at(position) {
                    return Err(WorldError::ContainerNotEmpty { object });
                }
                self.objects.remove_rooted_at(position);
            } else if previous == TileId::EMPTY {
                if let Some(object) = self.objects.occupying(position) {
                    if self.blocks_foreground_tile_placement(position) {
                        return Err(WorldError::OccupiedByObject { object });
                    }
                    let survives_placement = self.objects.object(object).is_some_and(|object| {
                        furniture_definition(object.object_type()).is_some()
                            || matches!(object.object_type(), ROPE_OBJECT | POWERED_CABLE_OBJECT)
                    });
                    if !survives_placement {
                        if !self.can_remove_object(object) {
                            return Err(WorldError::OccupiedByObject { object });
                        }
                        self.objects.remove_occupying(position);
                    }
                }
            } else if tile != ForegroundTile::GRASS {
                self.objects.remove_grass_dependent_rooted_at(position);
            }
        } else if tile == TileId::EMPTY
            && self.tile_in_bounds(x, y, Layer::Foreground) == TileId::EMPTY
        {
            self.objects.remove_rooted_at(TilePos::new(x, y));
        }
        self.clear_block_damage(TilePos::new(x, y), layer);
        self.chunks[chunk_index].set_tile(local_x, local_y, layer, tile);
        self.housing_changes.0 = self.housing_changes.0.wrapping_add(1);
        if layer == Layer::Foreground {
            self.foreground_changes.record(TilePos::new(x, y));
        }
        Ok(())
    }

    /// Removes one non-empty tile and returns everything detached by that
    /// destruction. Unlike the general editing API, this deliberately permits
    /// container furniture to be knocked off its support because the caller is
    /// given ownership of every stored stack in `unsupported_objects`.
    pub fn break_tile(
        &mut self,
        position: TilePos,
        layer: Layer,
    ) -> Result<Option<BrokenTile>, WorldError> {
        let (chunk_index, local_x, local_y) = self.locate(position.x, position.y)?;
        let tile = self.chunks[chunk_index].tile(local_x, local_y, layer);
        if tile == TileId::EMPTY {
            self.clear_block_damage(position, layer);
            return Ok(None);
        }
        let unsupported_objects = if layer == Layer::Foreground
            || self.tile_in_bounds(position.x, position.y, Layer::Foreground) == TileId::EMPTY
        {
            self.objects.remove_rooted_at(position)
        } else {
            Vec::new()
        };
        self.chunks[chunk_index].set_tile(local_x, local_y, layer, TileId::EMPTY);
        self.clear_block_damage(position, layer);
        self.housing_changes.0 = self.housing_changes.0.wrapping_add(1);
        if layer == Layer::Foreground {
            self.foreground_changes.record(position);
        }
        Ok(Some(BrokenTile {
            position,
            layer,
            tile,
            unsupported_objects,
        }))
    }

    /// Monotonically changes whenever a foreground cell is edited. Runtime
    /// read-only systems can use it to invalidate bounded derived caches.
    pub const fn foreground_revision(&self) -> u64 {
        self.foreground_changes.revision
    }

    pub(crate) const fn housing_revision(&self) -> [u64; 2] {
        [self.housing_changes.0, self.objects.spatial_revision()]
    }

    pub(crate) fn foreground_changes_since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = TilePos> + '_> {
        self.foreground_changes.since(revision)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), WorldError> {
        persistence::save(self, path.as_ref(), available_threads())
    }

    pub fn save_with_threads(
        &self,
        path: impl AsRef<Path>,
        threads: usize,
    ) -> Result<(), WorldError> {
        persistence::save(self, path.as_ref(), threads.max(1))
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, WorldError> {
        persistence::load(path.as_ref(), available_threads())
    }

    pub fn load_with_threads(path: impl AsRef<Path>, threads: usize) -> Result<Self, WorldError> {
        persistence::load(path.as_ref(), threads.max(1))
    }

    fn chunk_index(&self, pos: ChunkPos) -> Option<usize> {
        if pos.x >= self.chunks_wide || pos.y >= self.chunks_high {
            return None;
        }
        Some((pos.y * self.chunks_wide + pos.x) as usize)
    }

    fn locate(&self, x: u32, y: u32) -> Result<(usize, usize, usize), WorldError> {
        if x >= self.width || y >= self.height {
            return Err(WorldError::OutOfBounds {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }
        let chunk_x = x / CHUNK_SIZE as u32;
        let chunk_y = y / CHUNK_SIZE as u32;
        let index = (chunk_y * self.chunks_wide + chunk_x) as usize;
        Ok((index, x as usize % CHUNK_SIZE, y as usize % CHUNK_SIZE))
    }
}

#[derive(Debug)]
pub enum WorldError {
    InvalidDimensions {
        width: u32,
        height: u32,
    },
    OutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    ContainerNotEmpty {
        object: ObjectId,
    },
    OccupiedByObject {
        object: ObjectId,
    },
    InvalidData(String),
    WorkerPanicked,
    Io(io::Error),
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => write!(
                f,
                "invalid world dimensions {width}x{height}; expected 1..={MAX_WORLD_WIDTH} by 1..={MAX_WORLD_HEIGHT} and at most {MAX_WORLD_TILES} tiles"
            ),
            Self::OutOfBounds {
                x,
                y,
                width,
                height,
            } => write!(f, "tile ({x}, {y}) is outside world {width}x{height}"),
            Self::ContainerNotEmpty { object } => {
                write!(f, "container object {} is not empty", object.raw())
            }
            Self::OccupiedByObject { object } => {
                write!(f, "tile is occupied by object {}", object.raw())
            }
            Self::InvalidData(message) => write!(f, "invalid world data: {message}"),
            Self::WorkerPanicked => f.write_str("a terrain worker thread panicked"),
            Self::Io(error) => write!(f, "world I/O failed: {error}"),
        }
    }
}

impl Error for WorldError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for WorldError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), WorldError> {
    let tile_count = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_WORLD_WIDTH
        || height > MAX_WORLD_HEIGHT
        || tile_count > MAX_WORLD_TILES
    {
        return Err(WorldError::InvalidDimensions { width, height });
    }
    Ok(())
}

fn available_threads() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

fn parallel_mut<T, F>(values: &mut [T], threads: usize, operation: F) -> Result<(), WorldError>
where
    T: Send,
    F: Fn(usize, &mut T) + Sync,
{
    if values.is_empty() {
        return Ok(());
    }
    let workers = threads.max(1).min(values.len());
    let batch_size = values.len().div_ceil(workers);
    let result = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for (batch_index, batch) in values.chunks_mut(batch_size).enumerate() {
            let operation = &operation;
            handles.push(scope.spawn(move || {
                let base = batch_index * batch_size;
                for (offset, value) in batch.iter_mut().enumerate() {
                    operation(base + offset, value);
                }
            }));
        }
        handles.into_iter().all(|handle| handle.join().is_ok())
    });
    if result {
        Ok(())
    } else {
        Err(WorldError::WorkerPanicked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_worlds_begin_in_late_morning() {
        let world = World::empty(1, 1, 0).unwrap();
        assert_eq!(world.time_of_day(), DEFAULT_TIME_OF_DAY);
        assert!((0.30..0.50).contains(&world.time_of_day()));
    }

    #[test]
    fn elevation_is_positive_above_sea_level_and_negative_below() {
        let world = World::empty(10, 100, 0).unwrap();
        assert_eq!(world.sea_level_y(), 12);
        assert_eq!(world.elevation_decimetres(2.0), 70);
        assert_eq!(world.elevation_decimetres(12.0), 0);
        assert_eq!(world.elevation_decimetres(22.0), -70);
    }

    #[test]
    fn tile_access_crosses_chunk_boundaries_and_keeps_layers_separate() {
        let mut world = World::empty(65, 65, 9).unwrap();
        world
            .set_tile(64, 64, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        world
            .set_tile(64, 64, Layer::Background, BackgroundTile::DIRT_WALL)
            .unwrap();
        assert_eq!(
            world.tile(64, 64, Layer::Foreground).unwrap(),
            ForegroundTile::STONE
        );
        assert_eq!(
            world.tile(64, 64, Layer::Background).unwrap(),
            BackgroundTile::DIRT_WALL
        );
        assert_eq!(world.chunk_count(), 4);
        assert!(world.tile(65, 0, Layer::Foreground).is_err());
    }

    #[test]
    fn dimensions_are_bounded() {
        assert!(World::empty(0, 1, 0).is_err());
        assert!(World::empty(MAX_WORLD_WIDTH + 1, 1, 0).is_err());
        assert!(World::empty(1, MAX_WORLD_HEIGHT + 1, 0).is_err());
        assert!(validate_dimensions(MAX_WORLD_WIDTH, MAX_WORLD_HEIGHT).is_err());
        assert!(validate_dimensions(4_000, 16_000).is_ok());
    }

    #[test]
    #[ignore = "allocates the full supported 64-million-tile world"]
    fn maximum_supported_world_can_be_generated() {
        let world = WorldGenerator::new(99)
            .with_threads(4)
            .generate(4_000, 16_000)
            .unwrap();
        assert_eq!(world.chunk_count(), 15_750);
        assert_ne!(
            world.tile(3_999, 15_999, Layer::Foreground).unwrap(),
            ForegroundTile::AIR
        );
    }
}
