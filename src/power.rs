use crate::{
    BUILT_IN_FURNITURE, CargoLiftDirection, ChunkPos, FurnitureObject, Layer, ObjectId,
    POWERED_CABLE_OBJECT, PowerRole, TileId, TilePos, World,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const SOCKET_UNITS_PER_TILE: i64 = 2;
const CONNECTION_RANGE_SOCKET_UNITS: i64 = crate::POWER_CONNECTION_RANGE_HALF_TILES as i64;
const CONNECTION_RANGE_SQUARED: i64 = CONNECTION_RANGE_SOCKET_UNITS * CONNECTION_RANGE_SOCKET_UNITS;
pub const PYLON_CONNECTION_LIMIT: usize = 10;
pub const POWER_CONNECTOR_CONNECTION_LIMIT: usize = 5;
pub const MACHINE_CONNECTION_LIMIT: usize = 1;

/// One unobstructed automatic link. Coordinates use the same logical world
/// space as furniture and are consumed directly by the resident cable batch.
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
    role: PowerRole,
    socket: [i64; 2],
    rate_milli_per_second: u32,
    capacity_milli: u32,
    connection_limit: usize,
    connection_range_squared: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateConnection {
    nodes: [usize; 2],
    endpoints: [ObjectId; 2],
    socket_start: [i64; 2],
    socket_end: [i64; 2],
    crossed_tiles: Vec<TilePos>,
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
    candidates: Vec<CandidateConnection>,
    candidates_by_tile: HashMap<TilePos, Vec<usize>>,
    connections: Vec<PowerConnection>,
    connections_by_chunk: HashMap<ChunkPos, Vec<usize>>,
    powered_objects: HashSet<ObjectId>,
    previous_powered_objects: HashSet<ObjectId>,
    network_count: usize,
    network_nodes: Vec<Vec<usize>>,
    wired_edges: Vec<[usize; 2]>,
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
        let mut edges_rechecked = 0;
        let connections_changed;

        if topology_rebuilt {
            self.rebuild_candidates(world);
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
                let visible = has_line_of_sight(world, &candidate.crossed_tiles);
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
                match node.role {
                    PowerRole::Generator if daytime => {
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
                if node.role != PowerRole::Consumer {
                    if network_energized
                        && (node.role != PowerRole::Generator || has_live_generator)
                    {
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
        for cable in world.objects_of_type(POWERED_CABLE_OBJECT) {
            if world
                .powered_cable_anchor_ids(cable.id())
                .into_iter()
                .flatten()
                .any(|anchor| self.powered_objects.contains(&anchor))
            {
                self.powered_objects.insert(cable.id());
            }
        }
        if self.powered_objects != self.previous_powered_objects {
            self.revision = self.revision.wrapping_add(1);
        }
        flow
    }

    fn rebuild_candidates(&mut self, world: &World) {
        self.nodes.clear();
        for definition in BUILT_IN_FURNITURE
            .iter()
            .copied()
            .filter(|definition| definition.power_role().is_some())
        {
            let role = definition
                .power_role()
                .expect("filtered power furniture has a role");
            let offset = definition
                .power_socket_half_tiles()
                .expect("power furniture has a socket");
            for object in world.objects_of_type(definition.object_type()) {
                let anchor = object.anchor();
                self.nodes.push(PowerNode {
                    object: object.id(),
                    role,
                    socket: [
                        i64::from(anchor.x) * SOCKET_UNITS_PER_TILE + i64::from(offset[0]),
                        i64::from(anchor.y) * SOCKET_UNITS_PER_TILE + i64::from(offset[1]),
                    ],
                    rate_milli_per_second: definition.power_rate_milli_per_second(),
                    capacity_milli: definition.power_capacity_milli(),
                    connection_limit: match role {
                        PowerRole::Relay => usize::from(definition.power_connection_limit()),
                        PowerRole::Generator | PowerRole::Consumer | PowerRole::Storage => {
                            MACHINE_CONNECTION_LIMIT
                        }
                    },
                    connection_range_squared: match role {
                        PowerRole::Relay => {
                            let range = i64::from(definition.power_connection_range_half_tiles());
                            range * range
                        }
                        PowerRole::Generator | PowerRole::Consumer | PowerRole::Storage => {
                            CONNECTION_RANGE_SQUARED
                        }
                    },
                });
            }
        }
        self.nodes.sort_unstable_by_key(|node| node.object);
        let node_indices: HashMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.object, index))
            .collect();
        self.wired_edges.clear();
        for cable in world.objects_of_type(POWERED_CABLE_OBJECT) {
            let anchors: Vec<_> = world
                .powered_cable_anchor_ids(cable.id())
                .into_iter()
                .flatten()
                .filter_map(|anchor| node_indices.get(&anchor).copied())
                .collect();
            if let [top, bottom] = anchors.as_slice() {
                self.wired_edges.push([*top, *bottom]);
            }
        }
        for lift in world.objects_of_type(FurnitureObject::CARGO_LIFT) {
            let Some(lift_node) = node_indices.get(&lift.id()).copied() else {
                continue;
            };
            let Some(cable) = lift.linked_object() else {
                continue;
            };
            for anchor in world.powered_cable_anchor_ids(cable).into_iter().flatten() {
                if let Some(anchor_node) = node_indices.get(&anchor).copied() {
                    self.wired_edges.push([lift_node, anchor_node]);
                }
            }
        }
        for edge in &mut self.wired_edges {
            edge.sort_unstable();
        }
        self.wired_edges.sort_unstable();
        self.wired_edges.dedup();

        let mut buckets = HashMap::<[i64; 2], Vec<usize>>::new();
        for (index, node) in self.nodes.iter().enumerate() {
            buckets
                .entry(socket_bucket(node.socket))
                .or_default()
                .push(index);
        }

        self.candidates.clear();
        let mut seen = HashSet::<[ObjectId; 2]>::new();
        for (relay_index, relay) in self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.role == PowerRole::Relay)
        {
            let bucket = socket_bucket(relay.socket);
            for bucket_y in bucket[1] - 1..=bucket[1] + 1 {
                for bucket_x in bucket[0] - 1..=bucket[0] + 1 {
                    let Some(neighbours) = buckets.get(&[bucket_x, bucket_y]) else {
                        continue;
                    };
                    for &other_index in neighbours {
                        let other = self.nodes[other_index];
                        if other.object == relay.object
                            || !automatic_connection_allowed(world, relay.object, other.object)
                            || other.role == PowerRole::Relay && relay.object > other.object
                            || squared_socket_distance(relay.socket, other.socket)
                                > relay
                                    .connection_range_squared
                                    .min(other.connection_range_squared)
                        {
                            continue;
                        }
                        let mut endpoints = [relay.object, other.object];
                        endpoints.sort_unstable();
                        if !seen.insert(endpoints) {
                            continue;
                        }
                        let crossed_tiles = crossed_tiles(relay.socket, other.socket);
                        self.candidates.push(CandidateConnection {
                            nodes: [relay_index, other_index],
                            endpoints,
                            socket_start: relay.socket,
                            socket_end: other.socket,
                            visible: has_line_of_sight(world, &crossed_tiles),
                            connected: false,
                            crossed_tiles,
                        });
                    }
                }
            }
        }
        self.candidates
            .sort_unstable_by_key(|candidate| candidate.endpoints);

        self.candidates_by_tile.clear();
        for (index, candidate) in self.candidates.iter().enumerate() {
            for &tile in &candidate.crossed_tiles {
                self.candidates_by_tile.entry(tile).or_default().push(index);
            }
        }
        self.dirty_candidates.clear();
        self.dirty_candidate_marks.clear();
        self.dirty_candidate_marks
            .resize(self.candidates.len(), false);
    }

    fn collect_dirty_candidates(&mut self, world: &World) {
        self.dirty_candidates.clear();
        let Some(changes) = world.foreground_changes_since(self.known_foreground_revision) else {
            self.dirty_candidates.extend(0..self.candidates.len());
            self.dirty_candidate_marks.fill(true);
            return;
        };
        for position in changes {
            let Some(candidates) = self.candidates_by_tile.get(&position) else {
                continue;
            };
            for &candidate in candidates {
                if !self.dirty_candidate_marks[candidate] {
                    self.dirty_candidate_marks[candidate] = true;
                    self.dirty_candidates.push(candidate);
                }
            }
        }
    }

    fn rebuild_components(&mut self) {
        let mut union = UnionFind::new(self.nodes.len());
        let mut connected = vec![false; self.nodes.len()];
        let mut degrees = vec![0_usize; self.nodes.len()];
        for &[left, right] in &self.wired_edges {
            union.join(left, right);
            connected[left] = true;
            connected[right] = true;
        }
        let mut candidate_order: Vec<_> = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| candidate.visible.then_some(index))
            .collect();
        candidate_order.sort_unstable_by_key(|&index| {
            let candidate = &self.candidates[index];
            (
                squared_socket_distance(candidate.socket_start, candidate.socket_end),
                candidate.endpoints,
            )
        });
        for candidate in &mut self.candidates {
            candidate.connected = false;
        }
        for candidate_index in candidate_order {
            let [left, right] = self.candidates[candidate_index].nodes;
            if degrees[left] >= self.nodes[left].connection_limit
                || degrees[right] >= self.nodes[right].connection_limit
            {
                continue;
            }
            self.candidates[candidate_index].connected = true;
            degrees[left] += 1;
            degrees[right] += 1;
            union.join(left, right);
            connected[left] = true;
            connected[right] = true;
        }

        self.powered_objects.clear();
        self.network_nodes.clear();
        let mut networks_by_root = HashMap::new();
        for (index, is_connected) in connected.into_iter().enumerate() {
            if !is_connected {
                continue;
            }
            let root = union.root(index);
            let network = *networks_by_root.entry(root).or_insert_with(|| {
                self.network_nodes.push(Vec::new());
                self.network_nodes.len() - 1
            });
            self.network_nodes[network].push(index);
        }
        self.network_count = self.network_nodes.len();

        self.connections.clear();
        self.connections.extend(
            self.candidates
                .iter()
                .filter(|candidate| candidate.connected)
                .map(|candidate| PowerConnection {
                    endpoints: candidate.endpoints,
                    start: socket_position(candidate.socket_start),
                    end: socket_position(candidate.socket_end),
                }),
        );

        self.connections_by_chunk.clear();
        let connected_candidates = self
            .candidates
            .iter()
            .filter(|candidate| candidate.connected);
        for (connection_index, candidate) in connected_candidates.enumerate() {
            let mut chunks = Vec::with_capacity(4);
            for &tile in &candidate.crossed_tiles {
                let chunk = tile.chunk();
                if !chunks.contains(&chunk) {
                    chunks.push(chunk);
                }
            }
            for chunk in chunks {
                self.connections_by_chunk
                    .entry(chunk)
                    .or_default()
                    .push(connection_index);
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }
}

pub fn is_daytime(time_of_day: f32) -> bool {
    time_of_day.is_finite() && (0.25..0.75).contains(&time_of_day.rem_euclid(1.0))
}

fn energy_for_rate(rate_milli_per_second: u32, elapsed: Duration) -> u64 {
    let nanos = elapsed.as_nanos().min(u128::from(u64::MAX));
    (u128::from(rate_milli_per_second) * nanos / 1_000_000_000).min(u128::from(u64::MAX)) as u64
}

fn consumer_is_demanding(world: &World, object: ObjectId) -> bool {
    let Some(object) = world.object(object) else {
        return false;
    };
    match object.object_type() {
        crate::FurnitureObject::LASER_BORE | crate::FurnitureObject::TURRET => object.is_active(),
        crate::FurnitureObject::ORBITAL_EXPORT_LAUNCHER => {
            world
                .container(object.id())
                .is_some_and(|container| !container.is_empty())
                && world.orbital_export_has_sky_access(object.id())
        }
        crate::FurnitureObject::CARGO_LIFT => {
            world.cargo_lift_direction(object.id()) != Some(CargoLiftDirection::Idle)
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
    if left_type == FurnitureObject::CARGO_LIFT || right_type == FurnitureObject::CARGO_LIFT {
        return false;
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
mod tests {
    use super::*;
    use crate::{ForegroundTile, FurnitureObject};

    fn support(world: &mut World, x: u32, y: u32, width: u32) {
        for support_x in x..x + width {
            world
                .set_tile(support_x, y, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
    }

    fn basic_grid() -> (World, ObjectId, ObjectId, ObjectId) {
        let mut world = World::empty(64, 32, 0).unwrap();
        support(&mut world, 2, 12, 2);
        support(&mut world, 8, 12, 1);
        support(&mut world, 14, 12, 3);
        let solar = world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
            .unwrap();
        let pylon = world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(8, 10))
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(14, 9))
            .unwrap();
        (world, solar, pylon, bore)
    }

    #[test]
    fn pylon_energizes_a_visible_machine_from_a_visible_generator() {
        let (mut world, solar, pylon, bore) = basic_grid();
        let mut power = PowerSystem::new();
        let update = power.update(&world);
        let flow = power.distribute(&mut world, 0.5, Duration::from_secs(1));

        assert!(update.topology_rebuilt);
        assert_eq!(update.node_count, 3);
        assert_eq!(update.connection_count, 2);
        assert_eq!(update.network_count, 1);
        assert_eq!(flow.generated_milli, 12_000);
        assert!(power.is_powered(solar));
        assert!(power.is_powered(pylon));
        assert!(power.is_powered(bore));
    }

    #[test]
    fn generators_and_consumers_require_a_pylon_endpoint() {
        let mut world = World::empty(32, 20, 0).unwrap();
        support(&mut world, 2, 12, 2);
        support(&mut world, 8, 12, 3);
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(8, 9))
            .unwrap();

        let mut power = PowerSystem::new();
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(power.candidate_connection_count(), 0);
        assert_eq!(power.connection_count(), 0);
        assert!(!power.is_powered(bore));
    }

    #[test]
    fn range_is_inclusive_and_uses_socket_distance() {
        let mut world = World::empty(64, 32, 0).unwrap();
        support(&mut world, 2, 12, 1);
        support(&mut world, 24, 12, 1);
        support(&mut world, 47, 12, 1);
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(2, 10))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(24, 10))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(47, 10))
            .unwrap();

        let mut power = PowerSystem::new();
        power.update(&world);
        assert_eq!(power.candidate_connection_count(), 1);
        assert_eq!(power.connection_count(), 1);
    }

    #[test]
    fn foreground_los_changes_recheck_only_crossed_candidates() {
        let (mut world, _, _, bore) = basic_grid();
        support(&mut world, 50, 20, 1);
        let mut power = PowerSystem::new();
        power.update(&world);

        world
            .set_tile(11, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let blocked = power.update(&world);
        assert_eq!(blocked.edges_rechecked, 1);
        assert!(blocked.connections_changed);
        assert!(!power.is_powered(bore));
        assert_eq!(power.connection_count(), 1);

        world
            .set_tile(50, 20, Layer::Foreground, TileId::EMPTY)
            .unwrap();
        let unrelated = power.update(&world);
        assert_eq!(unrelated.edges_rechecked, 0);
        assert!(!unrelated.connections_changed);

        world
            .set_tile(11, 10, Layer::Foreground, TileId::EMPTY)
            .unwrap();
        let restored = power.update(&world);
        assert_eq!(restored.edges_rechecked, 1);
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert!(power.is_powered(bore));
    }

    #[test]
    fn diagonal_corner_blockers_prevent_connections() {
        let mut world = World::empty(32, 32, 0).unwrap();
        support(&mut world, 2, 16, 1);
        support(&mut world, 10, 8, 1);
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(2, 14))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(10, 6))
            .unwrap();
        world
            .set_tile(6, 9, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();

        let mut power = PowerSystem::new();
        power.update(&world);
        assert_eq!(power.candidate_connection_count(), 1);
        assert_eq!(power.connection_count(), 0);
    }

    #[test]
    fn power_propagates_across_multiple_pylons() {
        let mut world = World::empty(80, 32, 0).unwrap();
        support(&mut world, 2, 12, 2);
        support(&mut world, 14, 12, 1);
        support(&mut world, 28, 12, 1);
        support(&mut world, 40, 12, 3);
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 9))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(14, 10))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(28, 10))
            .unwrap();
        let bore = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(40, 9))
            .unwrap();

        let mut power = PowerSystem::new();
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(power.connection_count(), 3);
        assert!(power.is_powered(bore));
    }

    #[test]
    fn pylon_connections_are_limited_to_ten_nearest_nodes() {
        let mut world = World::empty(72, 24, 0).unwrap();
        support(&mut world, 30, 12, 1);
        let pylon = world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(30, 10))
            .unwrap();
        for x in [8, 11, 14, 17, 20, 23, 34, 37, 40, 43, 46] {
            support(&mut world, x, 12, 2);
            world
                .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(x, 9))
                .unwrap();
        }

        let mut power = PowerSystem::new();
        power.update(&world);
        let pylon_connections = power
            .connections()
            .iter()
            .filter(|connection| connection.endpoints().contains(&pylon))
            .count();

        assert_eq!(power.candidate_connection_count(), 11);
        assert_eq!(pylon_connections, PYLON_CONNECTION_LIMIT);
    }

    #[test]
    fn power_connector_has_five_connections() {
        let mut world = World::empty(48, 24, 0).unwrap();
        world
            .set_tile(20, 11, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let connector = world
            .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(20, 10))
            .unwrap();
        for x in [12, 14, 16, 21, 23, 25] {
            support(&mut world, x, 12, 2);
            world
                .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(x, 9))
                .unwrap();
        }
        let mut power = PowerSystem::new();
        power.update(&world);
        let connections: Vec<_> = power
            .connections()
            .iter()
            .filter(|connection| connection.endpoints().contains(&connector))
            .collect();

        assert_eq!(connections.len(), POWER_CONNECTOR_CONNECTION_LIMIT);
    }

    #[test]
    fn power_connector_range_is_eight_tiles() {
        let mut world = World::empty(32, 24, 0).unwrap();
        world
            .set_tile(10, 11, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let connector = world
            .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(10, 10))
            .unwrap();
        support(&mut world, 17, 12, 2);
        let nearby = world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(17, 9))
            .unwrap();
        support(&mut world, 19, 12, 2);
        let distant = world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(19, 9))
            .unwrap();

        let mut power = PowerSystem::new();
        power.update(&world);

        assert_eq!(power.candidate_connection_count(), 1);
        assert_eq!(power.connections().len(), 1);
        assert!(power.connections()[0].endpoints().contains(&connector));
        assert!(power.connections()[0].endpoints().contains(&nearby));
        assert!(!power.connections()[0].endpoints().contains(&distant));
    }

    #[test]
    fn pylon_connects_only_to_a_machine_without_an_existing_connection() {
        let mut world = World::empty(48, 24, 0).unwrap();
        support(&mut world, 20, 12, 2);
        let solar = world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(20, 9))
            .unwrap();
        for x in [7, 12, 28] {
            support(&mut world, x, 12, 1);
            world
                .place_furniture(FurnitureObject::PYLON, TilePos::new(x, 10))
                .unwrap();
        }

        let mut power = PowerSystem::new();
        power.update(&world);
        let machine_connections = power
            .connections()
            .iter()
            .filter(|connection| connection.endpoints().contains(&solar))
            .count();

        assert_eq!(machine_connections, MACHINE_CONNECTION_LIMIT);
    }

    #[test]
    fn unrelated_furniture_does_not_rebuild_power_topology() {
        let (mut world, _, _, _) = basic_grid();
        let mut power = PowerSystem::new();
        assert!(power.update(&world).topology_rebuilt);
        support(&mut world, 24, 12, 2);
        world
            .place_furniture(FurnitureObject::CHEST, TilePos::new(24, 10))
            .unwrap();
        assert!(!power.update(&world).topology_rebuilt);
    }

    #[test]
    fn socket_metadata_is_present_for_every_power_definition() {
        for definition in BUILT_IN_FURNITURE
            .iter()
            .copied()
            .filter(|definition| definition.power_role().is_some())
        {
            assert!(definition.power_socket_half_tiles().is_some());
            assert_eq!(
                crate::furniture_definition(definition.object_type()),
                Some(definition)
            );
        }
    }

    #[test]
    fn cable_chunk_index_returns_each_connection_once_per_chunk() {
        let (world, _, _, _) = basic_grid();
        let mut power = PowerSystem::new();
        power.update(&world);
        assert_eq!(
            power.connections_in_chunk(ChunkPos { x: 0, y: 0 }).count(),
            2
        );
        assert_eq!(crate::CHUNK_SIZE, 64);
    }

    #[test]
    fn solar_generation_stops_at_night() {
        let (mut world, solar, pylon, bore) = basic_grid();
        let mut power = PowerSystem::new();
        let flow = power.distribute(&mut world, 0.9, Duration::from_secs(1));

        assert!(!flow.daytime);
        assert_eq!(flow.generated_milli, 0);
        assert!(!power.is_powered(solar));
        assert!(!power.is_powered(pylon));
        assert!(!power.is_powered(bore));
    }

    #[test]
    fn battery_charges_by_day_and_supplies_an_active_bore_at_night() {
        let (mut world, _, _, bore) = basic_grid();
        support(&mut world, 10, 12, 2);
        let battery = world
            .place_furniture(FurnitureObject::BATTERY, TilePos::new(10, 10))
            .unwrap();
        let mut power = PowerSystem::new();

        let daylight = power.distribute(&mut world, 0.5, Duration::from_secs(10));
        assert_eq!(daylight.generated_milli, 120_000);
        assert_eq!(world.battery_charge_milli(battery), Some(120_000));

        assert!(world.set_furniture_active(bore, true));
        let night = power.distribute(&mut world, 0.9, Duration::from_secs(1));
        assert!(power.is_powered(bore));
        assert_eq!(night.consumed_milli, 8_000);
        assert_eq!(world.battery_charge_milli(battery), Some(112_000));

        power.distribute(&mut world, 0.9, Duration::from_secs(14));
        assert_eq!(world.battery_charge_milli(battery), Some(0));
        power.distribute(&mut world, 0.9, Duration::from_secs(1));
        assert!(!power.is_powered(bore));
    }

    #[test]
    fn insufficient_generation_load_sheds_consumers_in_stable_object_order() {
        let (mut world, _, _, bore) = basic_grid();
        support(&mut world, 18, 12, 2);
        let turret = world
            .place_furniture(FurnitureObject::TURRET, TilePos::new(18, 10))
            .unwrap();
        assert!(world.set_furniture_active(bore, true));
        assert!(world.set_furniture_active(turret, true));
        let mut power = PowerSystem::new();

        let flow = power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(flow.generated_milli, 12_000);
        assert_eq!(flow.consumed_milli, 8_000);
        assert!(power.is_powered(bore));
        assert!(!power.is_powered(turret));
    }

    #[test]
    fn bottom_anchor_conducts_power_to_a_lift_and_motion_consumes_energy() {
        let mut world = World::empty(40, 36, 0).unwrap();
        world
            .set_tile(12, 2, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let top_anchor = world
            .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(12, 3))
            .unwrap();
        for _ in 0..20 {
            world
                .place_or_extend_powered_cable(TilePos::new(12, 3))
                .unwrap();
        }
        let cable = world.object_at(TilePos::new(12, 8)).unwrap().id();
        let bottom_anchor = world
            .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(12, 24))
            .unwrap();
        let lift = world.place_cargo_lift(TilePos::new(12, 4)).unwrap();

        support(&mut world, 3, 24, 2);
        support(&mut world, 6, 24, 1);
        let solar = world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(3, 21))
            .unwrap();
        let pylon = world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 22))
            .unwrap();
        let mut power = PowerSystem::new();

        let update = power.update(&world);
        let idle = power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(update.connection_count, 3);
        assert_eq!(idle.consumed_milli, 0);
        assert!(power.is_powered(solar));
        assert!(power.is_powered(pylon));
        assert!(power.is_powered(bottom_anchor));
        assert!(power.is_powered(top_anchor));
        assert!(power.is_powered(cable));
        assert!(power.is_powered(lift));

        assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
        let moving = power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(moving.consumed_milli, 10_000);
        let registry = crate::ItemRegistry::with_built_ins();
        assert_eq!(
            world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
            1
        );
        assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 10.0);

        assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Up));
        let unpowered = power.distribute(&mut world, 0.9, Duration::from_secs(1));
        assert_eq!(unpowered.consumed_milli, 0);
        assert!(!power.is_powered(lift));
        assert_eq!(
            world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
            0
        );
        assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 10.0);
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Up)
        );

        let powered = power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(powered.consumed_milli, 10_000);
        assert_eq!(
            world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
            1
        );
        assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 4.0);
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Idle)
        );

        assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
        for _ in 0..3 {
            power.distribute(&mut world, 0.5, Duration::from_secs(1));
            world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
        }
        assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 22.0);
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Idle)
        );
    }

    #[test]
    fn lift_station_loads_unloads_and_dispatches_at_the_nearest_stop() {
        let mut world = World::empty(40, 36, 0).unwrap();
        world
            .set_tile(12, 2, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        world
            .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(12, 3))
            .unwrap();
        for _ in 0..20 {
            world
                .place_or_extend_powered_cable(TilePos::new(12, 3))
                .unwrap();
        }
        world
            .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(12, 24))
            .unwrap();
        let lift = world.place_cargo_lift(TilePos::new(12, 4)).unwrap();
        support(&mut world, 10, 12, 2);
        let station = world.place_lift_station(TilePos::new(10, 10)).unwrap();
        assert_eq!(
            world.object(station).unwrap().anchor(),
            TilePos::new(10, 10)
        );

        support(&mut world, 3, 24, 2);
        support(&mut world, 6, 24, 1);
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(3, 21))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 22))
            .unwrap();
        assert!(world.set_lift_station_departure(station, CargoLiftDirection::Up));

        let registry = crate::ItemRegistry::with_built_ins();
        let mut power = PowerSystem::new();
        assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(
            world.update_cargo_lifts(Duration::from_secs(1), &power, &registry),
            1
        );
        assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 10.0);
        assert!(world.container(lift).unwrap().is_empty());
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Idle)
        );

        // An empty lift waits for cargo instead of departing from an empty load station.
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Idle)
        );
        assert!(
            world
                .container_mut(station)
                .unwrap()
                .try_add(crate::ItemId::STONE_BLOCK, 37, 999)
        );

        let loading = power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(loading.consumed_milli, 0);
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
        assert_eq!(
            world.container(station).unwrap().slot(0),
            crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 17)
        );
        assert_eq!(
            world.container(lift).unwrap().slot(0),
            crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 20)
        );
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Idle)
        );

        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
        assert!(world.container(station).unwrap().is_empty());
        assert_eq!(
            world.container(lift).unwrap().slot(0),
            crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 37)
        );
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Up)
        );

        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
        assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 4.0);
        assert!(world.set_lift_station_mode(station, crate::LiftStationMode::Unload));
        assert!(world.set_lift_station_departure(station, CargoLiftDirection::Down));
        assert!(world.set_cargo_lift_direction(lift, CargoLiftDirection::Down));
        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);

        assert_eq!(world.container(station).unwrap().slot(0), None);
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Idle)
        );

        let unloading = power.distribute(&mut world, 0.5, Duration::from_secs(1));
        assert_eq!(unloading.consumed_milli, 0);
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
        assert_eq!(
            world.container(station).unwrap().slot(0),
            crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 20)
        );
        assert_eq!(
            world.container(lift).unwrap().slot(0),
            crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 17)
        );

        power.distribute(&mut world, 0.5, Duration::from_secs(1));
        world.update_cargo_lifts(Duration::from_secs(1), &power, &registry);
        assert!(world.container(lift).unwrap().is_empty());
        assert_eq!(
            world.container(station).unwrap().slot(0),
            crate::ItemStack::new(crate::ItemId::STONE_BLOCK, 37)
        );
        assert_eq!(
            world.cargo_lift_direction(lift),
            Some(CargoLiftDirection::Down)
        );
    }
}
