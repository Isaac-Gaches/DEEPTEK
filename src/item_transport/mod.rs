//! Revision-driven item transport networks and transfers.

use crate::machine_processing::{transfer_one_from_processor, transfer_one_to_processor};
use crate::{
    BUILT_IN_FURNITURE, ItemRegistry, ItemTransportRole, ObjectId, TilePos, World,
    furniture_definition,
};
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_ITEM_TRANSPORT_INTERVAL: Duration = Duration::from_millis(50);
const MAX_CATCH_UP_TICKS: usize = 20;
const ROUTE_PHASES: [(ItemTransportRole, ItemTransportRole); 7] = [
    (ItemTransportRole::Processor, ItemTransportRole::Input),
    (ItemTransportRole::Processor, ItemTransportRole::Buffer),
    (ItemTransportRole::Output, ItemTransportRole::Input),
    (ItemTransportRole::Output, ItemTransportRole::Processor),
    (ItemTransportRole::Output, ItemTransportRole::Buffer),
    (ItemTransportRole::Buffer, ItemTransportRole::Input),
    (ItemTransportRole::Buffer, ItemTransportRole::Processor),
];

/// The visual path through a one-tile item transport connector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemTransportShape {
    Horizontal,
    Vertical,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
}

/// Derives a connector's sprite from its current cardinal neighbours. This is
/// intentionally computed only while resident furniture instances are rebuilt;
/// the global transport simulation does not pay a per-frame visual cost.
pub fn item_transport_shape(world: &World, connector: ObjectId) -> Option<ItemTransportShape> {
    let object = world.object(connector)?;
    if !furniture_definition(object.object_type())?.is_item_transport_connector() {
        return None;
    }

    let anchor = object.anchor();
    let connected = |position: Option<TilePos>| {
        position
            .and_then(|position| world.object_at(position))
            .and_then(|neighbour| furniture_definition(neighbour.object_type()))
            .is_some_and(|definition| {
                definition.is_item_transport_connector()
                    || definition.interaction().item_transport_role().is_some()
            })
    };
    let west = connected(anchor.x.checked_sub(1).map(|x| TilePos::new(x, anchor.y)));
    let east = connected(
        anchor
            .x
            .checked_add(1)
            .filter(|&x| x < world.width())
            .map(|x| TilePos::new(x, anchor.y)),
    );
    let north = connected(anchor.y.checked_sub(1).map(|y| TilePos::new(anchor.x, y)));
    let south = connected(
        anchor
            .y
            .checked_add(1)
            .filter(|&y| y < world.height())
            .map(|y| TilePos::new(anchor.x, y)),
    );

    Some(shape_from_connections(north, east, south, west))
}

fn shape_from_connections(north: bool, east: bool, south: bool, west: bool) -> ItemTransportShape {
    match (north, east, south, west) {
        (true, true, false, false) => ItemTransportShape::NorthEast,
        (false, true, true, false) => ItemTransportShape::SouthEast,
        (false, false, true, true) => ItemTransportShape::SouthWest,
        (true, false, false, true) => ItemTransportShape::NorthWest,
        (true, false, true, false) => ItemTransportShape::Vertical,
        (false, _, true, false) | (true, false, false, false) => ItemTransportShape::Vertical,
        _ => ItemTransportShape::Horizontal,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ItemTransportUpdate {
    pub topology_rebuilt: bool,
    pub connector_count: usize,
    pub network_count: usize,
    pub transfer_count: usize,
    pub items_transferred: u64,
    pub processor_items_received: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TransportEndpoint {
    object: ObjectId,
    role: ItemTransportRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TransportNetwork {
    endpoints: Vec<TransportEndpoint>,
}

#[derive(Debug)]
pub struct ItemTransportSystem {
    interval_seconds: f64,
    accumulator_seconds: f64,
    known_topology_revision: Option<u64>,
    connector_count: usize,
    networks: Vec<TransportNetwork>,
}

impl ItemTransportSystem {
    pub fn new(interval: Duration) -> Self {
        assert!(
            !interval.is_zero(),
            "item transport interval must be non-zero"
        );
        Self {
            interval_seconds: interval.as_secs_f64(),
            accumulator_seconds: 0.0,
            known_topology_revision: None,
            connector_count: 0,
            networks: Vec::new(),
        }
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs_f64(self.interval_seconds)
    }

    pub fn connector_count(&self) -> usize {
        self.connector_count
    }

    pub fn network_count(&self) -> usize {
        self.networks.len()
    }

    /// Advances global cargo networks without consulting renderer residency.
    /// Topology is rebuilt only when connector or endpoint furniture changes;
    /// steady-state frames perform O(1) work until a transfer tick is due.
    pub fn update(
        &mut self,
        world: &mut World,
        registry: &ItemRegistry,
        elapsed_seconds: f32,
    ) -> ItemTransportUpdate {
        self.update_with_speed(world, registry, elapsed_seconds, 100)
    }

    pub fn update_with_speed(
        &mut self,
        world: &mut World,
        registry: &ItemRegistry,
        elapsed_seconds: f32,
        speed_percent: u16,
    ) -> ItemTransportUpdate {
        let revision = world.item_transport_revision();
        let topology_rebuilt = self.known_topology_revision != Some(revision);
        if topology_rebuilt {
            let (networks, connector_count) = build_networks(world);
            self.networks = networks;
            self.connector_count = connector_count;
            self.known_topology_revision = Some(revision);
        }

        if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            self.accumulator_seconds += f64::from(elapsed_seconds);
        }
        let effective_interval = self.interval_seconds * 100.0 / f64::from(speed_percent.max(1));
        let elapsed_ticks = (self.accumulator_seconds / effective_interval).floor() as usize;
        let ticks = elapsed_ticks.min(MAX_CATCH_UP_TICKS);
        if elapsed_ticks > MAX_CATCH_UP_TICKS {
            self.accumulator_seconds %= effective_interval;
        } else {
            self.accumulator_seconds -= ticks as f64 * effective_interval;
        }

        let mut update = ItemTransportUpdate {
            topology_rebuilt,
            connector_count: self.connector_count,
            network_count: self.networks.len(),
            ..ItemTransportUpdate::default()
        };
        for _ in 0..ticks {
            for network in &self.networks {
                if let Some(processor_received) = transfer_one_item(world, registry, network) {
                    update.transfer_count += 1;
                    update.items_transferred = update.items_transferred.saturating_add(1);
                    if processor_received {
                        update.processor_items_received =
                            update.processor_items_received.saturating_add(1);
                    }
                }
            }
        }
        update
    }
}

impl Default for ItemTransportSystem {
    fn default() -> Self {
        Self::new(DEFAULT_ITEM_TRANSPORT_INTERVAL)
    }
}

fn build_networks(world: &World) -> (Vec<TransportNetwork>, usize) {
    let mut connector_cells = HashMap::<TilePos, usize>::new();
    let mut connector_count = 0_usize;
    for definition in BUILT_IN_FURNITURE
        .iter()
        .copied()
        .filter(|definition| definition.is_item_transport_connector())
    {
        for object in world.objects_of_type(definition.object_type()) {
            let connector = connector_count;
            connector_count += 1;
            let anchor = object.anchor();
            let [width, height] = object.size();
            for offset_y in 0..u32::from(height) {
                for offset_x in 0..u32::from(width) {
                    connector_cells.insert(
                        TilePos::new(anchor.x + offset_x, anchor.y + offset_y),
                        connector,
                    );
                }
            }
        }
    }
    if connector_count == 0 {
        return (Vec::new(), 0);
    }

    let mut union = UnionFind::new(connector_count);
    for (&position, &connector) in &connector_cells {
        for neighbour in adjacent_positions(position, world.width(), world.height()) {
            if let Some(&other) = connector_cells.get(&neighbour) {
                union.join(connector, other);
            }
        }
    }

    let mut endpoint_links = HashMap::<ObjectId, EndpointLink>::new();
    for (&position, &connector) in &connector_cells {
        for neighbour in adjacent_positions(position, world.width(), world.height()) {
            if connector_cells.contains_key(&neighbour) {
                continue;
            }
            let Some(object) = world.object_at(neighbour) else {
                continue;
            };
            let Some(role) = furniture_definition(object.object_type())
                .and_then(|definition| definition.interaction().item_transport_role())
            else {
                continue;
            };
            let link = endpoint_links
                .entry(object.id())
                .or_insert_with(|| EndpointLink {
                    role,
                    connectors: Vec::new(),
                });
            debug_assert_eq!(link.role, role);
            if !link.connectors.contains(&connector) {
                link.connectors.push(connector);
            }
        }
    }

    for link in endpoint_links.values() {
        if let Some((&first, rest)) = link.connectors.split_first() {
            for &connector in rest {
                union.join(first, connector);
            }
        }
    }

    let mut grouped_endpoints = vec![Vec::<TransportEndpoint>::new(); connector_count];
    for (object, link) in endpoint_links {
        let Some(&connector) = link.connectors.first() else {
            continue;
        };
        let root = union.root(connector);
        grouped_endpoints[root].push(TransportEndpoint {
            object,
            role: link.role,
        });
    }

    let mut networks: Vec<_> = grouped_endpoints
        .into_iter()
        .filter_map(|mut endpoints| {
            endpoints.sort_unstable_by_key(|endpoint| endpoint.object);
            is_operational(&endpoints).then_some(TransportNetwork { endpoints })
        })
        .collect();
    networks.sort_unstable_by_key(|network| network.endpoints[0].object);
    (networks, connector_count)
}

fn is_operational(endpoints: &[TransportEndpoint]) -> bool {
    ROUTE_PHASES.iter().any(|&(source, destination)| {
        endpoints.iter().any(|endpoint| endpoint.role == source)
            && endpoints
                .iter()
                .any(|endpoint| endpoint.role == destination)
    })
}

fn transfer_one_item(
    world: &mut World,
    registry: &ItemRegistry,
    network: &TransportNetwork,
) -> Option<bool> {
    for (source_role, destination_role) in ROUTE_PHASES {
        for source in network
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.role == source_role)
        {
            if !machine_endpoint_is_operational(world, source.object) {
                continue;
            }
            let source_slots = world
                .container(source.object)
                .map_or(0, |container| container.slots().len());
            for slot in 0..source_slots {
                let Some(item) = world
                    .container(source.object)
                    .and_then(|container| container.slot(slot))
                    .map(|stack| stack.item())
                else {
                    continue;
                };
                let Some(max_stack) = registry.get(item).map(|definition| definition.max_stack)
                else {
                    continue;
                };
                for destination in network.endpoints.iter().filter(|endpoint| {
                    endpoint.role == destination_role && endpoint.object != source.object
                }) {
                    if !machine_endpoint_is_operational(world, destination.object) {
                        continue;
                    }
                    if destination.role == ItemTransportRole::Processor {
                        if transfer_one_to_processor(
                            world,
                            source.object,
                            destination.object,
                            registry,
                        ) {
                            return Some(true);
                        }
                        continue;
                    }
                    if source.role == ItemTransportRole::Processor {
                        if transfer_one_from_processor(
                            world,
                            source.object,
                            destination.object,
                            registry,
                        ) {
                            return Some(false);
                        }
                        continue;
                    }
                    let can_add = world
                        .container(destination.object)
                        .is_some_and(|container| container.can_add(item, 1, max_stack));
                    if !can_add {
                        continue;
                    }
                    if world.transfer_one_container_item(
                        source.object,
                        destination.object,
                        registry,
                    ) {
                        return Some(false);
                    }
                }
            }
        }
    }
    None
}

fn machine_endpoint_is_operational(world: &World, object: ObjectId) -> bool {
    world
        .machine_health(object)
        .is_none_or(|health| !health.is_disabled())
}

fn adjacent_positions(position: TilePos, width: u32, height: u32) -> impl Iterator<Item = TilePos> {
    [
        position
            .x
            .checked_sub(1)
            .map(|x| TilePos::new(x, position.y)),
        position
            .x
            .checked_add(1)
            .filter(|&x| x < width)
            .map(|x| TilePos::new(x, position.y)),
        position
            .y
            .checked_sub(1)
            .map(|y| TilePos::new(position.x, y)),
        position
            .y
            .checked_add(1)
            .filter(|&y| y < height)
            .map(|y| TilePos::new(position.x, y)),
    ]
    .into_iter()
    .flatten()
}

#[derive(Debug)]
struct EndpointLink {
    role: ItemTransportRole,
    connectors: Vec<usize>,
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
