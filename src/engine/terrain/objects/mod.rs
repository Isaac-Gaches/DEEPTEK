use super::{
    CHUNK_SIZE, ChunkPos, FurnitureBehavior, FurnitureDefinition, FurnitureObject,
    FurnitureSupport, NaturalObject, POWERED_CABLE_OBJECT, furniture_definition,
};
use crate::items::ItemContainer;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::ops::Bound::{Included, Unbounded};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct ObjectId(u64);

impl ObjectId {
    pub(super) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct ObjectTypeId(u16);

impl ObjectTypeId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TilePos {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineHealth {
    current: u16,
    maximum: u16,
}

impl MachineHealth {
    pub(super) const fn new(current: u16, maximum: u16) -> Self {
        Self { current, maximum }
    }

    pub const fn current(self) -> u16 {
        self.current
    }

    pub const fn maximum(self) -> u16 {
        self.maximum
    }

    pub const fn is_disabled(self) -> bool {
        self.current == 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MachineDamage {
    pub applied: u16,
    pub disabled: bool,
}

impl TilePos {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    pub const fn chunk(self) -> ChunkPos {
        ChunkPos {
            x: self.x / CHUNK_SIZE as u32,
            y: self.y / CHUNK_SIZE as u32,
        }
    }
}

/// Persistent anchored object data. Width/height describe occupied cells and are
/// intentionally generic enough for future furniture and interactable objects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldObject {
    pub(super) id: ObjectId,
    pub(super) object_type: ObjectTypeId,
    pub(super) anchor: TilePos,
    pub(super) root: TilePos,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) variant: u8,
    pub(super) growth_stage: u8,
    pub(super) active: bool,
    pub(super) health: u16,
    pub(super) stored_energy_milli: u32,
    pub(super) machine_target_y: u32,
    pub(super) kill_count: u32,
    pub(super) linked_object: u64,
    pub(super) motion_position_milli: u32,
    pub(super) next_update_tick: u64,
}

/// Complete result of removing one persistent object. Container contents are
/// transferred out atomically so gameplay code can emit them as world drops
/// without cloning or losing stacks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovedObject {
    object: WorldObject,
    contents: Vec<crate::ItemStack>,
}

impl RemovedObject {
    pub const fn object(&self) -> &WorldObject {
        &self.object
    }

    pub fn contents(&self) -> &[crate::ItemStack] {
        &self.contents
    }

    pub fn into_parts(self) -> (WorldObject, Vec<crate::ItemStack>) {
        (self.object, self.contents)
    }
}

impl WorldObject {
    pub const fn id(&self) -> ObjectId {
        self.id
    }

    pub const fn object_type(&self) -> ObjectTypeId {
        self.object_type
    }

    pub const fn anchor(&self) -> TilePos {
        self.anchor
    }

    pub const fn root(&self) -> TilePos {
        self.root
    }

    pub const fn size(&self) -> [u16; 2] {
        [self.width, self.height]
    }

    pub const fn variant(&self) -> u8 {
        self.variant
    }

    pub const fn growth_stage(&self) -> u8 {
        self.growth_stage
    }

    /// Raw definition-owned configuration persisted with the object. Callers
    /// should prefer a typed world accessor such as `furniture_target_priority`.
    pub const fn configuration(&self) -> u8 {
        self.variant
    }

    /// Whether an activatable object is currently running. Non-activatable
    /// objects remain active so existing growth and decoration behaviour keeps
    /// the same meaning.
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub const fn health(&self) -> u16 {
        self.health
    }

    pub const fn stored_energy_milli(&self) -> u32 {
        self.stored_energy_milli
    }

    pub const fn kill_count(&self) -> u32 {
        self.kill_count
    }

    pub const fn linked_object(&self) -> Option<ObjectId> {
        if self.linked_object == 0 {
            None
        } else {
            Some(ObjectId(self.linked_object))
        }
    }

    pub fn motion_position_tiles(&self) -> f32 {
        self.motion_position_milli as f32 / 1_000.0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Statistics for bounded, globally scheduled object work. The legacy type name
/// is retained because natural decoration growth was its original only caller.
pub struct DecorationUpdate {
    pub ticks_advanced: u64,
    pub objects_processed: usize,
    pub objects_grown: usize,
    pub tiles_destroyed: usize,
    pub budget_exhausted: bool,
    pub(super) changed_tiles: Vec<TilePos>,
    pub(super) detached_objects: Vec<RemovedObject>,
    pub(super) broken_tiles: Vec<super::BrokenTile>,
}

impl DecorationUpdate {
    pub fn changed_tiles(&self) -> &[TilePos] {
        &self.changed_tiles
    }

    pub fn detached_objects(&self) -> &[RemovedObject] {
        &self.detached_objects
    }

    pub fn broken_tiles(&self) -> &[super::BrokenTile] {
        &self.broken_tiles
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectPlacementError {
    OutOfBounds,
    Occupied(TilePos),
    RootIsEmpty(TilePos),
    MissingTransportConnection(TilePos),
    UnsupportedTransportJunction(TilePos),
    MissingPoweredCableAttachment(TilePos),
    UnsupportedType(ObjectTypeId),
}

impl fmt::Display for ObjectPlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds => f.write_str("object footprint or root is outside the world"),
            Self::Occupied(position) => {
                write!(
                    f,
                    "object cell ({}, {}) is occupied",
                    position.x, position.y
                )
            }
            Self::RootIsEmpty(position) => write!(
                f,
                "object root ({}, {}) has no foreground tile",
                position.x, position.y
            ),
            Self::MissingTransportConnection(position) => write!(
                f,
                "item transport connector ({}, {}) is not attached to a transport endpoint or line",
                position.x, position.y
            ),
            Self::UnsupportedTransportJunction(position) => write!(
                f,
                "item transport connector ({}, {}) would create an unsupported junction",
                position.x, position.y
            ),
            Self::MissingPoweredCableAttachment(position) => write!(
                f,
                "object at ({}, {}) is not attached to a powered cable or cable support",
                position.x, position.y
            ),
            Self::UnsupportedType(object_type) => {
                write!(f, "unsupported object type {}", object_type.raw())
            }
        }
    }
}

impl Error for ObjectPlacementError {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct GrowthEvent {
    pub(super) tick: u64,
    pub(super) object: ObjectId,
}

const POWER_TOPOLOGY_CHANGE_HISTORY: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PowerTopologyChange {
    pub object: ObjectId,
    pub object_type: ObjectTypeId,
}

#[derive(Clone, Debug)]
pub(super) struct ObjectStore {
    pub(super) objects: Vec<WorldObject>,
    pub(super) next_id: u64,
    revision: u64,
    spatial_revision: u64,
    item_transport_revision: u64,
    power_revision: u64,
    power_changes: VecDeque<PowerTopologyChange>,
    by_id: HashMap<ObjectId, usize>,
    by_type: HashMap<ObjectTypeId, Vec<ObjectId>>,
    by_root: HashMap<TilePos, Vec<ObjectId>>,
    occupancy: HashMap<TilePos, ObjectId>,
    structural_occupancy: HashSet<TilePos>,
    pub(super) containers: HashMap<ObjectId, ItemContainer>,
    cargo_lifts_by_cable: HashMap<ObjectId, ObjectId>,
    lift_stations_by_cable: HashMap<ObjectId, BTreeMap<u32, ObjectId>>,
    docked_lift_stations: HashMap<ObjectId, ObjectId>,
    lift_transfer_accumulators: HashMap<ObjectId, Duration>,
    by_chunk: Vec<Vec<ObjectId>>,
    schedule: BinaryHeap<Reverse<GrowthEvent>>,
    chunks_wide: u32,
}

impl PartialEq for ObjectStore {
    fn eq(&self, other: &Self) -> bool {
        self.objects == other.objects
            && self.next_id == other.next_id
            && self.containers == other.containers
    }
}

impl Eq for ObjectStore {}

impl ObjectStore {
    pub(super) fn new(chunk_count: usize, chunks_wide: u32) -> Self {
        Self {
            objects: Vec::new(),
            next_id: 1,
            revision: 0,
            spatial_revision: 0,
            item_transport_revision: 0,
            power_revision: 0,
            power_changes: VecDeque::new(),
            by_id: HashMap::new(),
            by_type: HashMap::new(),
            by_root: HashMap::new(),
            occupancy: HashMap::new(),
            structural_occupancy: HashSet::new(),
            containers: HashMap::new(),
            cargo_lifts_by_cable: HashMap::new(),
            lift_stations_by_cable: HashMap::new(),
            docked_lift_stations: HashMap::new(),
            lift_transfer_accumulators: HashMap::new(),
            by_chunk: vec![Vec::new(); chunk_count],
            schedule: BinaryHeap::new(),
            chunks_wide,
        }
    }

    pub(super) fn allocate_id(&mut self) -> ObjectId {
        let id = ObjectId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub(super) fn insert(&mut self, object: WorldObject) -> Result<(), ObjectPlacementError> {
        for cell in object_cells(&object) {
            if self.occupancy.contains_key(&cell) {
                return Err(ObjectPlacementError::Occupied(cell));
            }
        }
        let changes_item_transport = affects_item_transport_topology(&object);
        let changes_power = affects_power_topology(&object);
        let structural = furniture_definition(object.object_type)
            .is_some_and(FurnitureDefinition::is_structural);
        let lift_station_link = (object.object_type == FurnitureObject::LIFT_STATION)
            .then(|| object.linked_object().map(|cable| (cable, object.anchor.y)))
            .flatten();
        let cargo_lift_link = furniture_definition(object.object_type)
            .is_some_and(|definition| definition.behavior() == FurnitureBehavior::CargoLift)
            .then(|| object.linked_object())
            .flatten();
        let index = self.objects.len();
        let id = object.id;
        let object_type = object.object_type;
        self.by_id.insert(id, index);
        self.by_type.entry(object.object_type).or_default().push(id);
        for support in object_support_cells(&object) {
            self.by_root.entry(support).or_default().push(id);
        }
        for cell in object_cells(&object) {
            self.occupancy.insert(cell, id);
            if structural {
                self.structural_occupancy.insert(cell);
            }
        }
        for chunk in covered_chunks(&object) {
            let index = (chunk.y * self.chunks_wide + chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(index) {
                objects.push(id);
            }
        }
        if object.next_update_tick != u64::MAX {
            self.schedule.push(Reverse(GrowthEvent {
                tick: object.next_update_tick,
                object: id,
            }));
        }
        self.objects.push(object);
        if let Some(cable) = cargo_lift_link {
            self.cargo_lifts_by_cable.insert(cable, id);
        }
        if let Some((cable, height)) = lift_station_link {
            self.lift_stations_by_cable
                .entry(cable)
                .or_default()
                .insert(height, id);
        }
        self.revision = self.revision.wrapping_add(1);
        self.spatial_revision = self.spatial_revision.wrapping_add(1);
        if changes_item_transport {
            self.item_transport_revision = self.item_transport_revision.wrapping_add(1);
        }
        if changes_power {
            self.record_power_change(id, object_type);
        }
        Ok(())
    }

    pub(super) fn object(&self, id: ObjectId) -> Option<&WorldObject> {
        self.by_id.get(&id).map(|&index| &self.objects[index])
    }

    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) const fn spatial_revision(&self) -> u64 {
        self.spatial_revision
    }

    pub(super) const fn item_transport_revision(&self) -> u64 {
        self.item_transport_revision
    }

    pub(super) const fn power_revision(&self) -> u64 {
        self.power_revision
    }

    pub(super) fn power_changes_since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = PowerTopologyChange> + '_> {
        if revision > self.power_revision {
            return None;
        }
        let retained = self.power_changes.len() as u64;
        let oldest = self
            .power_revision
            .saturating_sub(retained)
            .saturating_add(1);
        if revision < oldest.saturating_sub(1) {
            return None;
        }
        let skip = revision.saturating_add(1).saturating_sub(oldest) as usize;
        Some(self.power_changes.iter().skip(skip).copied())
    }

    fn record_power_change(&mut self, object: ObjectId, object_type: ObjectTypeId) {
        self.power_revision = self.power_revision.saturating_add(1);
        if self.power_changes.len() == POWER_TOPOLOGY_CHANGE_HISTORY {
            self.power_changes.pop_front();
        }
        self.power_changes.push_back(PowerTopologyChange {
            object,
            object_type,
        });
    }

    pub(super) fn mark_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn mark_spatial_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.spatial_revision = self.spatial_revision.wrapping_add(1);
    }

    pub(super) fn object_mut(&mut self, id: ObjectId) -> Option<&mut WorldObject> {
        let index = *self.by_id.get(&id)?;
        Some(&mut self.objects[index])
    }

    pub(super) fn occupying(&self, position: TilePos) -> Option<ObjectId> {
        self.occupancy.get(&position).copied()
    }

    pub(super) fn ids_in_chunk(&self, position: ChunkPos) -> &[ObjectId] {
        let index = (position.y * self.chunks_wide + position.x) as usize;
        self.by_chunk.get(index).map(Vec::as_slice).unwrap_or(&[])
    }

    pub(super) fn ids_of_type(&self, object_type: ObjectTypeId) -> &[ObjectId] {
        self.by_type
            .get(&object_type)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn lift_station_at(&self, cable: ObjectId, height: u32) -> Option<ObjectId> {
        self.lift_stations_by_cable
            .get(&cable)?
            .get(&height)
            .copied()
    }

    pub(super) fn cargo_lift_for_cable(&self, cable: ObjectId) -> Option<ObjectId> {
        self.cargo_lifts_by_cable.get(&cable).copied()
    }

    pub(super) fn next_lift_station(
        &self,
        cable: ObjectId,
        position_milli: u32,
        upward: bool,
    ) -> Option<ObjectId> {
        let stations = self.lift_stations_by_cable.get(&cable)?;
        if upward {
            let maximum = position_milli.saturating_sub(1) / 1_000;
            stations
                .range((Unbounded, Included(maximum)))
                .next_back()
                .map(|(_, &station)| station)
        } else {
            let minimum = position_milli / 1_000 + 1;
            stations
                .range((Included(minimum), Unbounded))
                .next()
                .map(|(_, &station)| station)
        }
    }

    pub(super) fn has_lift_station_for_cable(&self, cable: ObjectId) -> bool {
        self.lift_stations_by_cable
            .get(&cable)
            .is_some_and(|stations| !stations.is_empty())
    }

    pub(super) fn set_docked_station(&mut self, lift: ObjectId, station: ObjectId) {
        if self.docked_lift_stations.insert(lift, station) != Some(station) {
            self.lift_transfer_accumulators.insert(lift, Duration::ZERO);
        }
    }

    pub(super) fn lift_transfer_ticks(
        &mut self,
        lift: ObjectId,
        elapsed: Duration,
        interval: Duration,
        maximum: usize,
    ) -> usize {
        let accumulator = self
            .lift_transfer_accumulators
            .entry(lift)
            .or_insert(Duration::ZERO);
        *accumulator = accumulator.saturating_add(elapsed);
        let interval_nanos = interval.as_nanos();
        let elapsed_ticks = accumulator.as_nanos() / interval_nanos;
        let ticks = elapsed_ticks.min(maximum as u128) as usize;
        let remainder = if elapsed_ticks > maximum as u128 {
            accumulator.as_nanos() % interval_nanos
        } else {
            accumulator.as_nanos() - ticks as u128 * interval_nanos
        };
        *accumulator = Duration::from_nanos(remainder as u64);
        ticks
    }

    pub(super) fn clear_docked_station(&mut self, lift: ObjectId) {
        self.docked_lift_stations.remove(&lift);
        self.lift_transfer_accumulators.remove(&lift);
    }

    pub(super) fn invalidate_station_dock(&mut self, station: ObjectId) {
        let accumulators = &mut self.lift_transfer_accumulators;
        self.docked_lift_stations.retain(|lift, docked| {
            if *docked == station {
                accumulators.remove(lift);
                false
            } else {
                true
            }
        });
    }

    pub(super) fn remove_rooted_at(&mut self, position: TilePos) -> Vec<RemovedObject> {
        let Some(objects) = self.by_root.get(&position).cloned() else {
            return Vec::new();
        };
        let mut removed = Vec::new();
        for object in objects {
            removed.extend(self.remove_with_dependents(object));
        }
        removed
    }

    /// Removes furniture rooted on any occupied cell before removing the support.
    /// The root index bounds this to the actual dependency tree rather than all objects.
    pub(super) fn remove_with_dependents(&mut self, id: ObjectId) -> Vec<RemovedObject> {
        let Some(cells) = self
            .object(id)
            .map(|object| object_cells(object).collect::<Vec<_>>())
        else {
            return Vec::new();
        };
        let mut removed = Vec::new();
        for cell in cells {
            let Some(dependents) = self.by_root.get(&cell).cloned() else {
                continue;
            };
            for dependent in dependents {
                if dependent != id {
                    removed.extend(self.remove_with_dependents(dependent));
                }
            }
        }
        if let Some(object) = self.remove(id) {
            removed.push(object);
        }
        removed
    }

    /// Cuts a rope at `cut_y`, removing that segment and everything below it
    /// while retaining the upper portion as the original persistent object.
    pub(super) fn remove_rope_from(&mut self, id: ObjectId, cut_y: u32) -> Vec<RemovedObject> {
        let Some(object) = self.object(id) else {
            return Vec::new();
        };
        if object.object_type != super::ROPE_OBJECT
            || object.width != 1
            || cut_y < object.anchor.y
            || cut_y >= object.anchor.y + u32::from(object.height)
        {
            return Vec::new();
        }
        let mut contiguous_lower = Vec::new();
        let mut next_y = object.anchor.y + u32::from(object.height);
        while let Some(lower) = self
            .occupying(TilePos::new(object.anchor.x, next_y))
            .and_then(|lower| self.object(lower))
            .filter(|lower| lower.object_type == super::ROPE_OBJECT && lower.anchor.y == next_y)
        {
            contiguous_lower.push(lower.id);
            next_y = lower.anchor.y + u32::from(lower.height);
        }
        if cut_y == object.anchor.y {
            let mut removed = self.remove_with_dependents(id);
            for lower in contiguous_lower {
                removed.extend(self.remove_with_dependents(lower));
            }
            return removed;
        }

        let bottom = object.anchor.y + u32::from(object.height);
        let lower_cells: Vec<_> = (cut_y..bottom)
            .map(|y| TilePos::new(object.anchor.x, y))
            .collect();
        let mut removed = Vec::new();
        for cell in &lower_cells {
            let Some(dependents) = self.by_root.get(cell).cloned() else {
                continue;
            };
            for dependent in dependents {
                if dependent != id {
                    removed.extend(self.remove_with_dependents(dependent));
                }
            }
        }
        for lower in contiguous_lower {
            removed.extend(self.remove_with_dependents(lower));
        }

        let Some(index) = self.by_id.get(&id).copied() else {
            return removed;
        };
        let old_chunks: HashSet<_> = covered_chunks(&self.objects[index]).collect();
        let retained_height = (cut_y - self.objects[index].anchor.y) as u16;
        let removed_height = self.objects[index].height - retained_height;
        let mut lower = self.objects[index].clone();
        lower.anchor.y = cut_y;
        lower.root = lower.anchor;
        lower.height = removed_height;
        self.objects[index].height = retained_height;

        for cell in lower_cells {
            self.occupancy.remove(&cell);
            self.structural_occupancy.remove(&cell);
        }
        let retained_chunks: HashSet<_> = covered_chunks(&self.objects[index]).collect();
        for chunk in old_chunks.difference(&retained_chunks) {
            let chunk_index = (chunk.y * self.chunks_wide + chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(chunk_index) {
                objects.retain(|&candidate| candidate != id);
            }
        }
        self.revision = self.revision.wrapping_add(1);
        self.spatial_revision = self.spatial_revision.wrapping_add(1);
        removed.push(RemovedObject {
            object: lower,
            contents: Vec::new(),
        });
        removed
    }

    pub(super) fn has_dependents(&self, id: ObjectId) -> bool {
        self.object(id).is_some_and(|object| {
            object_cells(object).any(|cell| {
                self.by_root
                    .get(&cell)
                    .is_some_and(|objects| objects.iter().any(|&candidate| candidate != id))
            })
        })
    }

    pub(super) fn remove_grass_dependent_rooted_at(&mut self, position: TilePos) -> usize {
        let Some(objects) = self.by_root.get(&position).cloned() else {
            return 0;
        };
        let mut objects = objects;
        objects.retain(|&id| {
            self.object(id).is_some_and(|object| {
                matches!(
                    object.object_type,
                    NaturalObject::GRASS | NaturalObject::VINE
                )
            })
        });
        let count = objects.len();
        for object in objects {
            self.remove(object);
        }
        count
    }

    pub(super) fn remove_occupying(&mut self, position: TilePos) -> bool {
        let Some(object) = self.occupying(position) else {
            return false;
        };
        self.remove(object).is_some()
    }

    pub(super) fn remove(&mut self, id: ObjectId) -> Option<RemovedObject> {
        let index = self.by_id.remove(&id)?;
        let object = self.objects.swap_remove(index);
        let contents = self
            .containers
            .remove(&id)
            .map(|container| container.slots().iter().flatten().copied().collect())
            .unwrap_or_default();
        let changes_item_transport = affects_item_transport_topology(&object);
        let changes_power = affects_power_topology(&object);
        let structural = furniture_definition(object.object_type)
            .is_some_and(FurnitureDefinition::is_structural);
        if index < self.objects.len() {
            self.by_id.insert(self.objects[index].id, index);
        }
        if let Some(objects) = self.by_type.get_mut(&object.object_type) {
            objects.retain(|&candidate| candidate != id);
            if objects.is_empty() {
                self.by_type.remove(&object.object_type);
            }
        }
        if object.object_type == FurnitureObject::LIFT_STATION
            && let Some(cable) = object.linked_object()
            && let Some(stations) = self.lift_stations_by_cable.get_mut(&cable)
        {
            stations.remove(&object.anchor.y);
            if stations.is_empty() {
                self.lift_stations_by_cable.remove(&cable);
            }
        }
        if furniture_definition(object.object_type)
            .is_some_and(|definition| definition.behavior() == FurnitureBehavior::CargoLift)
        {
            if let Some(cable) = object.linked_object() {
                self.cargo_lifts_by_cable.remove(&cable);
            }
            self.clear_docked_station(id);
        } else if object.object_type == FurnitureObject::LIFT_STATION {
            self.invalidate_station_dock(id);
        }
        for support in object_support_cells(&object) {
            if let Some(objects) = self.by_root.get_mut(&support) {
                objects.retain(|&candidate| candidate != id);
                if objects.is_empty() {
                    self.by_root.remove(&support);
                }
            }
        }
        for cell in object_cells(&object) {
            self.occupancy.remove(&cell);
            if structural {
                self.structural_occupancy.remove(&cell);
            }
        }
        for chunk in covered_chunks(&object) {
            let chunk_index = (chunk.y * self.chunks_wide + chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(chunk_index) {
                objects.retain(|&candidate| candidate != id);
            }
        }
        self.revision = self.revision.wrapping_add(1);
        self.spatial_revision = self.spatial_revision.wrapping_add(1);
        if changes_item_transport {
            self.item_transport_revision = self.item_transport_revision.wrapping_add(1);
        }
        if changes_power {
            self.record_power_change(id, object.object_type);
        }
        Some(RemovedObject { object, contents })
    }

    pub(super) fn has_non_empty_container_rooted_at(&self, position: TilePos) -> Option<ObjectId> {
        self.by_root.get(&position)?.iter().copied().find(|id| {
            self.containers
                .get(id)
                .is_some_and(|container| !container.is_empty())
        })
    }

    #[inline]
    pub(super) fn structural_at(&self, position: TilePos) -> bool {
        !self.structural_occupancy.is_empty() && self.structural_occupancy.contains(&position)
    }

    pub(super) fn pop_due(&mut self, current_tick: u64) -> Option<GrowthEvent> {
        while let Some(Reverse(event)) = self.schedule.peek().copied() {
            if event.tick > current_tick {
                return None;
            }
            self.schedule.pop();
            if self
                .object(event.object)
                .is_some_and(|object| object.next_update_tick == event.tick)
            {
                return Some(event);
            }
        }
        None
    }

    pub(super) fn has_due(&mut self, current_tick: u64) -> bool {
        while let Some(Reverse(event)) = self.schedule.peek().copied() {
            if event.tick > current_tick {
                return false;
            }
            if self
                .object(event.object)
                .is_some_and(|object| object.next_update_tick == event.tick)
            {
                return true;
            }
            self.schedule.pop();
        }
        false
    }

    pub(super) fn schedule(&mut self, id: ObjectId, tick: u64) {
        let Some(object) = self.object_mut(id) else {
            return;
        };
        object.next_update_tick = tick;
        if tick != u64::MAX {
            self.schedule
                .push(Reverse(GrowthEvent { tick, object: id }));
        }
    }

    pub(super) fn extend_down(&mut self, id: ObjectId) -> bool {
        let Some(index) = self.by_id.get(&id).copied() else {
            return false;
        };
        let object = &self.objects[index];
        let changes_power = affects_power_topology(object);
        let new_cell = TilePos {
            x: object.anchor.x,
            y: object.anchor.y + u32::from(object.height),
        };
        if self.occupancy.contains_key(&new_cell) {
            return false;
        }
        let old_last_chunk = TilePos {
            x: object.anchor.x,
            y: new_cell.y - 1,
        }
        .chunk();
        let new_chunk = new_cell.chunk();
        self.objects[index].height += 1;
        let object_type = self.objects[index].object_type;
        self.occupancy.insert(new_cell, id);
        self.revision = self.revision.wrapping_add(1);
        if changes_power {
            self.record_power_change(id, object_type);
        }
        if new_chunk != old_last_chunk {
            let chunk_index = (new_chunk.y * self.chunks_wide + new_chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(chunk_index) {
                objects.push(id);
            }
        }
        true
    }

    pub(super) fn merge_down(&mut self, upper: ObjectId, lower: ObjectId) -> bool {
        let Some(upper_object) = self.object(upper) else {
            return false;
        };
        let Some(lower_object) = self.object(lower) else {
            return false;
        };
        if upper == lower
            || upper_object.object_type != lower_object.object_type
            || upper_object.anchor.x != lower_object.anchor.x
            || upper_object.anchor.y + u32::from(upper_object.height) != lower_object.anchor.y
        {
            return false;
        }
        let lower_height = lower_object.height;
        if self.remove(lower).is_none() {
            return false;
        }
        for _ in 0..lower_height {
            let extended = self.extend_down(upper);
            debug_assert!(extended, "validated vertical object merge must extend");
            if !extended {
                return false;
            }
        }
        true
    }

    /// Relocates free-standing moving furniture without invalidating power or
    /// item-network topology. Cargo-lift wiring is object-ID based, so crossing
    /// a tile or chunk boundary changes only occupancy and render residency.
    pub(super) fn relocate_free(&mut self, id: ObjectId, anchor: TilePos) -> bool {
        let Some(index) = self.by_id.get(&id).copied() else {
            return false;
        };
        let object = &self.objects[index];
        if object.anchor == anchor
            || furniture_definition(object.object_type)
                .is_none_or(|definition| definition.support() != FurnitureSupport::Free)
        {
            return false;
        }
        for offset_y in 0..u32::from(object.height) {
            for offset_x in 0..u32::from(object.width) {
                let cell = TilePos::new(anchor.x + offset_x, anchor.y + offset_y);
                if self
                    .occupancy
                    .get(&cell)
                    .is_some_and(|&occupant| occupant != id)
                {
                    return false;
                }
            }
        }

        let old_chunks: Vec<_> = covered_chunks(object).collect();
        let old_cells: Vec<_> = object_cells(object).collect();
        for cell in old_cells {
            self.occupancy.remove(&cell);
        }
        for chunk in old_chunks {
            let chunk_index = (chunk.y * self.chunks_wide + chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(chunk_index) {
                objects.retain(|&candidate| candidate != id);
            }
        }

        self.objects[index].anchor = anchor;
        self.objects[index].root = anchor;
        let object = &self.objects[index];
        for cell in object_cells(object) {
            self.occupancy.insert(cell, id);
        }
        for chunk in covered_chunks(object) {
            let chunk_index = (chunk.y * self.chunks_wide + chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(chunk_index)
                && !objects.contains(&id)
            {
                objects.push(id);
            }
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

fn affects_item_transport_topology(object: &WorldObject) -> bool {
    furniture_definition(object.object_type).is_some_and(|definition| {
        definition.is_item_transport_connector()
            || definition.interaction().item_transport_role().is_some()
    })
}

fn affects_power_topology(object: &WorldObject) -> bool {
    object.object_type == POWERED_CABLE_OBJECT
        || furniture_definition(object.object_type)
            .is_some_and(|definition| definition.power_role().is_some())
}

fn object_cells(object: &WorldObject) -> impl Iterator<Item = TilePos> + '_ {
    (0..u32::from(object.height)).flat_map(move |offset_y| {
        (0..u32::from(object.width)).map(move |offset_x| TilePos {
            x: object.anchor.x + offset_x,
            y: object.anchor.y + offset_y,
        })
    })
}

fn object_support_cells(object: &WorldObject) -> impl Iterator<Item = TilePos> + '_ {
    let (support, support_width) = furniture_definition(object.object_type)
        .map(|definition| (definition.support(), object.width))
        .unwrap_or((FurnitureSupport::Floor, 1));
    (0..support_width)
        .filter(move |&column| support.requires_column(column, support_width))
        .map(move |column| TilePos {
            x: object.root.x + u32::from(column),
            y: object.root.y,
        })
}

fn covered_chunks(object: &WorldObject) -> impl Iterator<Item = ChunkPos> {
    let first = object.anchor.chunk();
    let last = TilePos {
        x: object.anchor.x + u32::from(object.width) - 1,
        y: object.anchor.y + u32::from(object.height) - 1,
    }
    .chunk();
    (first.y..=last.y).flat_map(move |y| (first.x..=last.x).map(move |x| ChunkPos { x, y }))
}

#[cfg(test)]
mod tests;
