use super::{
    CHUNK_SIZE, ChunkPos, FurnitureObject, FurnitureSupport, NaturalObject, POWERED_CABLE_OBJECT,
    furniture_definition,
};
use crate::items::ItemContainer;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
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
    pub(super) stored_energy_milli: u32,
    pub(super) machine_target_y: u32,
    pub(super) kill_count: u32,
    pub(super) linked_object: u64,
    pub(super) motion_position_milli: u32,
    pub(super) next_update_tick: u64,
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
}

impl DecorationUpdate {
    pub fn changed_tiles(&self) -> &[TilePos] {
        &self.changed_tiles
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

#[derive(Clone, Debug)]
pub(super) struct ObjectStore {
    pub(super) objects: Vec<WorldObject>,
    pub(super) next_id: u64,
    revision: u64,
    item_transport_revision: u64,
    power_revision: u64,
    by_id: HashMap<ObjectId, usize>,
    by_type: HashMap<ObjectTypeId, Vec<ObjectId>>,
    by_root: HashMap<TilePos, Vec<ObjectId>>,
    occupancy: HashMap<TilePos, ObjectId>,
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
            item_transport_revision: 0,
            power_revision: 0,
            by_id: HashMap::new(),
            by_type: HashMap::new(),
            by_root: HashMap::new(),
            occupancy: HashMap::new(),
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
        let lift_station_link = (object.object_type == FurnitureObject::LIFT_STATION)
            .then(|| object.linked_object().map(|cable| (cable, object.anchor.y)))
            .flatten();
        let cargo_lift_link = (object.object_type == FurnitureObject::CARGO_LIFT)
            .then(|| object.linked_object())
            .flatten();
        let index = self.objects.len();
        let id = object.id;
        self.by_id.insert(id, index);
        self.by_type.entry(object.object_type).or_default().push(id);
        for support in object_support_cells(&object) {
            self.by_root.entry(support).or_default().push(id);
        }
        for cell in object_cells(&object) {
            self.occupancy.insert(cell, id);
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
        if changes_item_transport {
            self.item_transport_revision = self.item_transport_revision.wrapping_add(1);
        }
        if changes_power {
            self.power_revision = self.power_revision.wrapping_add(1);
        }
        Ok(())
    }

    pub(super) fn object(&self, id: ObjectId) -> Option<&WorldObject> {
        self.by_id.get(&id).map(|&index| &self.objects[index])
    }

    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) const fn item_transport_revision(&self) -> u64 {
        self.item_transport_revision
    }

    pub(super) const fn power_revision(&self) -> u64 {
        self.power_revision
    }

    pub(super) fn mark_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
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

    pub(super) fn remove_rooted_at(&mut self, position: TilePos) -> usize {
        let Some(objects) = self.by_root.get(&position).cloned() else {
            return 0;
        };
        let count = objects.len();
        for object in objects {
            self.remove(object);
        }
        count
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

    pub(super) fn remove(&mut self, id: ObjectId) -> Option<WorldObject> {
        if self
            .containers
            .get(&id)
            .is_some_and(|container| !container.is_empty())
        {
            return None;
        }
        let index = self.by_id.remove(&id)?;
        let object = self.objects.swap_remove(index);
        let changes_item_transport = affects_item_transport_topology(&object);
        let changes_power = affects_power_topology(&object);
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
        if object.object_type == FurnitureObject::CARGO_LIFT {
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
        }
        for chunk in covered_chunks(&object) {
            let chunk_index = (chunk.y * self.chunks_wide + chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(chunk_index) {
                objects.retain(|&candidate| candidate != id);
            }
        }
        self.revision = self.revision.wrapping_add(1);
        if changes_item_transport {
            self.item_transport_revision = self.item_transport_revision.wrapping_add(1);
        }
        if changes_power {
            self.power_revision = self.power_revision.wrapping_add(1);
        }
        self.containers.remove(&id);
        Some(object)
    }

    pub(super) fn has_non_empty_container_rooted_at(&self, position: TilePos) -> Option<ObjectId> {
        self.by_root.get(&position)?.iter().copied().find(|id| {
            self.containers
                .get(id)
                .is_some_and(|container| !container.is_empty())
        })
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
        self.occupancy.insert(new_cell, id);
        self.revision = self.revision.wrapping_add(1);
        if changes_power {
            self.power_revision = self.power_revision.wrapping_add(1);
        }
        if new_chunk != old_last_chunk {
            let chunk_index = (new_chunk.y * self.chunks_wide + new_chunk.x) as usize;
            if let Some(objects) = self.by_chunk.get_mut(chunk_index) {
                objects.push(id);
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
