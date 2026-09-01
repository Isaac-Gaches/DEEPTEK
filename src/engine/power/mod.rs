mod topology;

// Sparse power-grid topology, distribution, and storage simulation.

use crate::{
    BUILT_IN_FURNITURE, CargoLiftDirection, ChunkPos, FurnitureObject, Layer, ObjectId,
    ObjectTypeId, POWERED_CABLE_OBJECT, PowerRole, TileId, TilePos, World,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const SOCKET_UNITS_PER_TILE: i64 = 2;
const CONNECTION_RANGE_SOCKET_UNITS: i64 = crate::POWER_CONNECTION_RANGE_HALF_TILES as i64;
const CONNECTION_RANGE_SQUARED: i64 = CONNECTION_RANGE_SOCKET_UNITS * CONNECTION_RANGE_SOCKET_UNITS;
pub const PYLON_CONNECTION_LIMIT: usize = 10;
pub const POWER_CONNECTOR_CONNECTION_LIMIT: usize = 5;
pub const MACHINE_CONNECTION_LIMIT: usize = 1;
const POWERED_CABLE_CONNECTION_LIMIT: usize = 4;
const POWERED_CABLE_ENDPOINT_CONNECTION_LIMIT: usize = 2;
const MAX_NODE_SOCKETS: usize = 2;

/// One automatic link. Pylon links require a clear path; links involving a
/// wall-mounted connector can cross foreground blocks. Coordinates use the
/// same logical world space as furniture and feed the resident cable batch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerConnection {
    endpoints: [ObjectId; 2],
    start: [f32; 2],
    end: [f32; 2],
}

impl PowerConnection {
    pub const fn endpoints(self) -> [ObjectId; 2] {
        self.endpoints
    }

    pub const fn start(self) -> [f32; 2] {
        self.start
    }

    pub const fn end(self) -> [f32; 2] {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PowerUpdate {
    pub topology_rebuilt: bool,
    pub full_topology_rebuild: bool,
    pub topology_changes_applied: usize,
    pub edges_rechecked: usize,
    pub connections_changed: bool,
    pub node_count: usize,
    pub candidate_connection_count: usize,
    pub connection_count: usize,
    pub network_count: usize,
    pub powered_object_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PowerFlow {
    pub daytime: bool,
    pub generated_milli: u64,
    pub consumed_milli: u64,
    pub stored_milli: u64,
    pub curtailed_milli: u64,
    pub supplied_consumers: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PowerNode {
    object: ObjectId,
    object_type: crate::ObjectTypeId,
    role: PowerRole,
    sockets: [[i64; 2]; MAX_NODE_SOCKETS],
    socket_count: u8,
    rate_milli_per_second: u32,
    capacity_milli: u32,
    connection_limit: usize,
    socket_connection_limit: usize,
    connection_range_squared: i64,
}

impl PowerNode {
    fn sockets(self) -> impl Iterator<Item = (u8, [i64; 2])> {
        self.sockets
            .into_iter()
            .take(usize::from(self.socket_count))
            .enumerate()
            .map(|(index, socket)| (index as u8, socket))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PowerSocketRef {
    object: ObjectId,
    index: u8,
    position: [i64; 2],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateConnection {
    endpoints: [ObjectId; 2],
    endpoint_sockets: [u8; 2],
    socket_start: [i64; 2],
    socket_end: [i64; 2],
    crossed_tiles: Vec<TilePos>,
    passes_through_foreground: bool,
    visible: bool,
    connected: bool,
}

/// Sparse, revision-driven power graph. Furniture changes rebuild candidates
/// through fixed-size spatial buckets. Foreground edits recheck only candidate
/// links whose short LOS traversal includes the edited tile.
#[derive(Debug, Default)]
pub struct PowerSystem {
    known_power_revision: Option<u64>,
    known_foreground_revision: u64,
    revision: u64,
    nodes: Vec<PowerNode>,
    node_indices: HashMap<ObjectId, usize>,
    node_buckets: HashMap<[i64; 2], Vec<PowerSocketRef>>,
    candidates: Vec<CandidateConnection>,
    candidates_by_tile: HashMap<TilePos, Vec<usize>>,
    connections: Vec<PowerConnection>,
    connections_by_chunk: HashMap<ChunkPos, Vec<usize>>,
    powered_objects: HashSet<ObjectId>,
    previous_powered_objects: HashSet<ObjectId>,
    network_count: usize,
    network_nodes: Vec<Vec<usize>>,
    wired_edges: Vec<[ObjectId; 2]>,
    battery_scratch: Vec<(ObjectId, u32)>,
    dirty_candidates: Vec<usize>,
    dirty_candidate_marks: Vec<bool>,
}

impl PowerSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_powered(&self, object: ObjectId) -> bool {
        self.powered_objects.contains(&object)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn candidate_connection_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub const fn network_count(&self) -> usize {
        self.network_count
    }

    pub fn connections(&self) -> &[PowerConnection] {
        &self.connections
    }

    pub fn connections_in_chunk(&self, chunk: ChunkPos) -> impl Iterator<Item = &PowerConnection> {
        self.connections_by_chunk
            .get(&chunk)
            .into_iter()
            .flatten()
            .filter_map(|&index| self.connections.get(index))
    }

    pub fn update(&mut self, world: &World) -> PowerUpdate {
        let power_revision = world.power_revision();
        let topology_rebuilt = self.known_power_revision != Some(power_revision);
        let mut full_topology_rebuild = false;
        let mut topology_changes_applied = 0;
        let mut edges_rechecked = 0;
        let connections_changed;

        if topology_rebuilt {
            let incrementally_updated = if let Some(known_revision) = self.known_power_revision {
                world
                    .power_changes_since(known_revision)
                    .map(|changes| {
                        let changes: Vec<_> = changes
                            .map(|change| (change.object, change.object_type))
                            .collect();
                        topology_changes_applied = changes.len();
                        self.apply_topology_changes(world, &changes)
                    })
                    .unwrap_or(false)
            } else {
                false
            };
            if !incrementally_updated {
                full_topology_rebuild = true;
                topology_changes_applied = 0;
                self.rebuild_candidates(world);
            } else if self.known_foreground_revision != world.foreground_revision() {
                self.collect_dirty_candidates(world);
                for scratch_index in 0..self.dirty_candidates.len() {
                    let candidate_index = self.dirty_candidates[scratch_index];
                    self.dirty_candidate_marks[candidate_index] = false;
                    let candidate = &mut self.candidates[candidate_index];
                    candidate.visible = candidate_is_visible(world, candidate);
                    edges_rechecked += 1;
                }
                self.dirty_candidates.clear();
            }
            self.known_power_revision = Some(power_revision);
            self.known_foreground_revision = world.foreground_revision();
            self.rebuild_components();
            connections_changed = true;
        } else if self.known_foreground_revision != world.foreground_revision() {
            self.collect_dirty_candidates(world);
            let mut visibility_changed = false;
            for scratch_index in 0..self.dirty_candidates.len() {
                let candidate_index = self.dirty_candidates[scratch_index];
                self.dirty_candidate_marks[candidate_index] = false;
                let candidate = &mut self.candidates[candidate_index];
                let visible = candidate_is_visible(world, candidate);
                visibility_changed |= visible != candidate.visible;
                candidate.visible = visible;
                edges_rechecked += 1;
            }
            self.dirty_candidates.clear();
            self.known_foreground_revision = world.foreground_revision();
            if visibility_changed {
                self.rebuild_components();
            }
            connections_changed = visibility_changed;
        } else {
            connections_changed = false;
        }

        PowerUpdate {
            topology_rebuilt,
            full_topology_rebuild,
            topology_changes_applied,
            edges_rechecked,
            connections_changed,
            node_count: self.nodes.len(),
            candidate_connection_count: self.candidates.len(),
            connection_count: self.connections.len(),
            network_count: self.network_count,
            powered_object_count: self.powered_objects.len(),
        }
    }

    pub fn distribute(
        &mut self,
        world: &mut World,
        time_of_day: f32,
        elapsed: Duration,
    ) -> PowerFlow {
        self.update(world);
        std::mem::swap(
            &mut self.powered_objects,
            &mut self.previous_powered_objects,
        );
        self.powered_objects.clear();

        let daytime = is_daytime(time_of_day);
        let mut flow = PowerFlow {
            daytime,
            ..PowerFlow::default()
        };
        for network_index in 0..self.network_nodes.len() {
            self.battery_scratch.clear();
            let mut generated = 0_u64;
            let mut stored = 0_u64;
            let mut has_live_generator = false;
            for &node_index in &self.network_nodes[network_index] {
                let node = self.nodes[node_index];
                if world
                    .machine_health(node.object)
                    .is_some_and(|health| health.is_disabled())
                {
                    continue;
                }
                match node.role {
                    PowerRole::Generator if generator_is_live(world, node, daytime) => {
                        has_live_generator = true;
                        generated = generated
                            .saturating_add(energy_for_rate(node.rate_milli_per_second, elapsed));
                    }
                    PowerRole::Storage => {
                        let charge = world.battery_charge_milli(node.object).unwrap_or(0);
                        stored = stored.saturating_add(u64::from(charge));
                        self.battery_scratch
                            .push((node.object, node.capacity_milli));
                    }
                    PowerRole::Generator | PowerRole::Relay | PowerRole::Consumer => {}
                }
            }

            let network_energized = has_live_generator || stored > 0;
            let mut available = generated.saturating_add(stored);
            let mut consumed = 0_u64;
            for &node_index in &self.network_nodes[network_index] {
                let node = self.nodes[node_index];
                if world
                    .machine_health(node.object)
                    .is_some_and(|health| health.is_disabled())
                {
                    continue;
                }
                if node.role != PowerRole::Consumer {
                    if node.role == PowerRole::Generator {
                        if generator_is_live(world, node, daytime) {
                            self.powered_objects.insert(node.object);
                        }
                    } else if network_energized {
                        self.powered_objects.insert(node.object);
                    }
                    continue;
                }
                if !consumer_is_demanding(world, node.object) {
                    if network_energized {
                        self.powered_objects.insert(node.object);
                    }
                    continue;
                }
                let demand = energy_for_rate(node.rate_milli_per_second, elapsed);
                if network_energized && available >= demand {
                    available -= demand;
                    consumed = consumed.saturating_add(demand);
                    flow.supplied_consumers += 1;
                    self.powered_objects.insert(node.object);
                }
            }

            if consumed > generated {
                discharge_batteries(
                    world,
                    &self.battery_scratch,
                    consumed.saturating_sub(generated),
                );
            } else {
                let surplus = generated - consumed;
                let accepted = charge_batteries(world, &self.battery_scratch, surplus);
                flow.curtailed_milli = flow
                    .curtailed_milli
                    .saturating_add(surplus.saturating_sub(accepted));
            }
            flow.generated_milli = flow.generated_milli.saturating_add(generated);
            flow.consumed_milli = flow.consumed_milli.saturating_add(consumed);
        }
        flow.stored_milli = self
            .nodes
            .iter()
            .filter(|node| node.role == PowerRole::Storage)
            .filter_map(|node| world.battery_charge_milli(node.object))
            .map(u64::from)
            .sum();
        if self.powered_objects != self.previous_powered_objects {
            self.revision = self.revision.wrapping_add(1);
        }
        flow
    }
}

pub fn is_daytime(time_of_day: f32) -> bool {
    time_of_day.is_finite() && (0.25..0.75).contains(&time_of_day.rem_euclid(1.0))
}

fn energy_for_rate(rate_milli_per_second: u32, elapsed: Duration) -> u64 {
    let nanos = elapsed.as_nanos().min(u128::from(u64::MAX));
    (u128::from(rate_milli_per_second) * nanos / 1_000_000_000).min(u128::from(u64::MAX)) as u64
}

fn generator_is_live(world: &World, node: PowerNode, daytime: bool) -> bool {
    daytime
        && (node.object_type != FurnitureObject::SOLAR_ARRAY
            || world.solar_array_has_sky_access(node.object))
}

fn consumer_is_demanding(world: &World, object: ObjectId) -> bool {
    let Some(object) = world.object(object) else {
        return false;
    };
    let definition = crate::furniture_definition(object.object_type());
    if matches!(
        definition.map(|value| value.behavior()),
        Some(crate::FurnitureBehavior::CargoLift)
    ) {
        return world.cargo_lift_direction(object.id()) != Some(CargoLiftDirection::Idle);
    }
    if definition.is_some_and(|value| value.interaction().is_activatable()) {
        return object.is_active();
    }
    match object.object_type() {
        crate::FurnitureObject::ORBITAL_EXPORT_LAUNCHER => {
            world
                .container(object.id())
                .is_some_and(|container| !container.is_empty())
                && world.orbital_export_has_sky_access(object.id())
        }
        _ => true,
    }
}

fn automatic_connection_allowed(world: &World, left: ObjectId, right: ObjectId) -> bool {
    let Some(left_type) = world.object(left).map(|object| object.object_type()) else {
        return false;
    };
    let Some(right_type) = world.object(right).map(|object| object.object_type()) else {
        return false;
    };
    if [left_type, right_type].into_iter().any(|object_type| {
        crate::furniture_definition(object_type).is_some_and(|definition| {
            matches!(definition.behavior(), crate::FurnitureBehavior::CargoLift)
        })
    }) {
        return false;
    }
    if left_type == POWERED_CABLE_OBJECT || right_type == POWERED_CABLE_OBJECT {
        return matches!(
            (left_type, right_type),
            (
                POWERED_CABLE_OBJECT,
                FurnitureObject::PYLON | FurnitureObject::POWER_CONNECTOR
            ) | (
                FurnitureObject::PYLON | FurnitureObject::POWER_CONNECTOR,
                POWERED_CABLE_OBJECT
            )
        );
    }
    if left_type == FurnitureObject::POWERED_CABLE_ANCHOR
        || right_type == FurnitureObject::POWERED_CABLE_ANCHOR
    {
        return matches!(
            (left_type, right_type),
            (
                FurnitureObject::POWERED_CABLE_ANCHOR,
                FurnitureObject::PYLON | FurnitureObject::POWER_CONNECTOR
            ) | (
                FurnitureObject::PYLON | FurnitureObject::POWER_CONNECTOR,
                FurnitureObject::POWERED_CABLE_ANCHOR
            )
        );
    }
    true
}

fn discharge_batteries(world: &mut World, batteries: &[(ObjectId, u32)], mut amount: u64) {
    for &(battery, _) in batteries {
        if amount == 0 {
            break;
        }
        let charge = world.battery_charge_milli(battery).unwrap_or(0);
        let taken = u64::from(charge).min(amount) as u32;
        if taken > 0 {
            world.set_battery_charge_milli(battery, charge - taken);
            amount -= u64::from(taken);
        }
    }
}

fn charge_batteries(world: &mut World, batteries: &[(ObjectId, u32)], mut amount: u64) -> u64 {
    let initial = amount;
    for &(battery, capacity) in batteries {
        if amount == 0 {
            break;
        }
        let charge = world.battery_charge_milli(battery).unwrap_or(0);
        let accepted = u64::from(capacity.saturating_sub(charge)).min(amount) as u32;
        if accepted > 0 {
            world.set_battery_charge_milli(battery, charge + accepted);
            amount -= u64::from(accepted);
        }
    }
    initial - amount
}

fn socket_bucket(socket: [i64; 2]) -> [i64; 2] {
    [
        socket[0].div_euclid(CONNECTION_RANGE_SOCKET_UNITS),
        socket[1].div_euclid(CONNECTION_RANGE_SOCKET_UNITS),
    ]
}

fn squared_socket_distance(left: [i64; 2], right: [i64; 2]) -> i64 {
    let dx = right[0] - left[0];
    let dy = right[1] - left[1];
    dx * dx + dy * dy
}

fn socket_position(socket: [i64; 2]) -> [f32; 2] {
    [
        socket[0] as f32 / SOCKET_UNITS_PER_TILE as f32,
        socket[1] as f32 / SOCKET_UNITS_PER_TILE as f32,
    ]
}

/// Supercover traversal between socket positions. It includes both neighbours
/// when the segment crosses a tile corner, so diagonal corner gaps cannot leak
/// a power connection.
fn crossed_tiles(start: [i64; 2], end: [i64; 2]) -> Vec<TilePos> {
    const ENDPOINT_NUDGE: f32 = 0.0001;
    let socket_start = socket_position(start);
    let socket_end = socket_position(end);
    let raw_delta = [
        socket_end[0] - socket_start[0],
        socket_end[1] - socket_start[1],
    ];
    let step = [sight_step(raw_delta[0]), sight_step(raw_delta[1])];
    // A definition socket may lie exactly on a tile boundary. Nudge both ends
    // into the open segment so the traversal cannot step away from an endpoint
    // cell at t=1 and then chase an unreachable cell forever.
    let start_position = [
        socket_start[0] + step[0] as f32 * ENDPOINT_NUDGE,
        socket_start[1] + step[1] as f32 * ENDPOINT_NUDGE,
    ];
    let end_position = [
        socket_end[0] - step[0] as f32 * ENDPOINT_NUDGE,
        socket_end[1] - step[1] as f32 * ENDPOINT_NUDGE,
    ];
    let mut cell = sight_cell(start_position);
    let end_cell = sight_cell(end_position);
    let mut output = Vec::with_capacity(crate::POWER_CONNECTION_RANGE_TILES as usize + 4);
    push_cell(&mut output, cell);
    if cell == end_cell {
        return output;
    }

    let delta = [
        end_position[0] - start_position[0],
        end_position[1] - start_position[1],
    ];
    let mut maximum_t = [f32::INFINITY; 2];
    let mut delta_t = [f32::INFINITY; 2];
    for axis in 0..2 {
        if step[axis] == 0 {
            continue;
        }
        let boundary = cell[axis] as f32 + step[axis] as f32 * 0.5;
        maximum_t[axis] = (boundary - start_position[axis]) / delta[axis];
        delta_t[axis] = 1.0 / delta[axis].abs();
    }

    let mut traversals = 0;
    while cell != end_cell && traversals < 128 {
        traversals += 1;
        match maximum_t[0].total_cmp(&maximum_t[1]) {
            std::cmp::Ordering::Less => {
                cell[0] += i64::from(step[0]);
                maximum_t[0] += delta_t[0];
                push_cell(&mut output, cell);
            }
            std::cmp::Ordering::Greater => {
                cell[1] += i64::from(step[1]);
                maximum_t[1] += delta_t[1];
                push_cell(&mut output, cell);
            }
            std::cmp::Ordering::Equal => {
                let horizontal = [cell[0] + i64::from(step[0]), cell[1]];
                let vertical = [cell[0], cell[1] + i64::from(step[1])];
                push_cell(&mut output, horizontal);
                push_cell(&mut output, vertical);
                cell = [horizontal[0], vertical[1]];
                maximum_t[0] += delta_t[0];
                maximum_t[1] += delta_t[1];
                push_cell(&mut output, cell);
            }
        }
    }
    debug_assert_eq!(
        cell, end_cell,
        "bounded power LOS traversal reached its cap"
    );
    output
}

fn sight_cell(position: [f32; 2]) -> [i64; 2] {
    [
        (position[0] + 0.5).floor() as i64,
        (position[1] + 0.5).floor() as i64,
    ]
}

fn sight_step(delta: f32) -> i32 {
    if delta > 0.0 {
        1
    } else if delta < 0.0 {
        -1
    } else {
        0
    }
}

fn push_cell(output: &mut Vec<TilePos>, cell: [i64; 2]) {
    let (Ok(x), Ok(y)) = (u32::try_from(cell[0]), u32::try_from(cell[1])) else {
        return;
    };
    let position = TilePos::new(x, y);
    if !output.contains(&position) {
        output.push(position);
    }
}

fn has_line_of_sight(world: &World, crossed_tiles: &[TilePos]) -> bool {
    crossed_tiles.iter().all(|position| {
        position.x < world.width()
            && position.y < world.height()
            && world.tile_in_bounds(position.x, position.y, Layer::Foreground) == TileId::EMPTY
    })
}

fn candidate_is_visible(world: &World, candidate: &CandidateConnection) -> bool {
    candidate.passes_through_foreground || has_line_of_sight(world, &candidate.crossed_tiles)
}

#[derive(Debug)]
struct UnionFind {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl UnionFind {
    fn new(length: usize) -> Self {
        Self {
            parents: (0..length).collect(),
            ranks: vec![0; length],
        }
    }

    fn root(&mut self, index: usize) -> usize {
        let parent = self.parents[index];
        if parent != index {
            self.parents[index] = self.root(parent);
        }
        self.parents[index]
    }

    fn join(&mut self, left: usize, right: usize) {
        let mut left_root = self.root(left);
        let mut right_root = self.root(right);
        if left_root == right_root {
            return;
        }
        if self.ranks[left_root] < self.ranks[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parents[right_root] = left_root;
        if self.ranks[left_root] == self.ranks[right_root] {
            self.ranks[left_root] = self.ranks[left_root].saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests;
