use super::*;

impl PowerSystem {
    pub(super) fn rebuild_candidates(&mut self, world: &World) {
        self.nodes.clear();
        self.node_indices.clear();
        self.node_buckets.clear();
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
                let connection_limit = match role {
                    PowerRole::Relay => usize::from(definition.power_connection_limit()),
                    PowerRole::Generator | PowerRole::Consumer | PowerRole::Storage => {
                        MACHINE_CONNECTION_LIMIT
                    }
                };
                self.nodes.push(PowerNode {
                    object: object.id(),
                    object_type: object.object_type(),
                    role,
                    sockets: [
                        [
                            i64::from(anchor.x) * SOCKET_UNITS_PER_TILE + i64::from(offset[0]),
                            i64::from(anchor.y) * SOCKET_UNITS_PER_TILE + i64::from(offset[1]),
                        ],
                        [0; 2],
                    ],
                    socket_count: 1,
                    rate_milli_per_second: definition.power_rate_milli_per_second(),
                    capacity_milli: definition.power_capacity_milli(),
                    connection_limit,
                    socket_connection_limit: connection_limit,
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
        self.nodes.extend(
            world
                .objects_of_type(POWERED_CABLE_OBJECT)
                .map(powered_cable_node),
        );
        self.nodes.sort_unstable_by_key(|node| node.object);
        for (index, node) in self.nodes.iter().enumerate() {
            self.node_indices.insert(node.object, index);
            for (socket_index, socket) in node.sockets() {
                self.node_buckets
                    .entry(socket_bucket(socket))
                    .or_default()
                    .push(PowerSocketRef {
                        object: node.object,
                        index: socket_index,
                        position: socket,
                    });
            }
        }
        self.wired_edges.clear();
        for definition in crate::BUILT_IN_FURNITURE.iter().filter(|definition| {
            matches!(definition.behavior(), crate::FurnitureBehavior::CargoLift)
        }) {
            for lift in world.objects_of_type(definition.object_type()) {
                if !self.node_indices.contains_key(&lift.id()) {
                    continue;
                }
                let Some(cable) = lift.linked_object() else {
                    continue;
                };
                if self.node_indices.contains_key(&cable) {
                    self.wired_edges.push([lift.id(), cable]);
                }
            }
        }
        for edge in &mut self.wired_edges {
            edge.sort_unstable();
        }
        self.wired_edges.sort_unstable();
        self.wired_edges.dedup();

        self.candidates.clear();
        let mut seen = HashSet::<[ObjectId; 2]>::new();
        let mut cable_candidates = HashMap::<[ObjectId; 2], usize>::new();
        for relay in self
            .nodes
            .iter()
            .filter(|node| node.role == PowerRole::Relay)
        {
            for (relay_socket_index, relay_socket) in relay.sockets() {
                let bucket = socket_bucket(relay_socket);
                for bucket_y in bucket[1] - 1..=bucket[1] + 1 {
                    for bucket_x in bucket[0] - 1..=bucket[0] + 1 {
                        let Some(neighbours) = self.node_buckets.get(&[bucket_x, bucket_y]) else {
                            continue;
                        };
                        for &other_socket in neighbours {
                            let other_id = other_socket.object;
                            let other_index = self.node_indices[&other_id];
                            let other = self.nodes[other_index];
                            if other.object == relay.object
                                || !automatic_connection_allowed(world, relay.object, other.object)
                                || other.role == PowerRole::Relay && relay.object > other.object
                                || squared_socket_distance(relay_socket, other_socket.position)
                                    > connection_range_squared(relay, &other)
                            {
                                continue;
                            }
                            let mut endpoints = [relay.object, other.object];
                            let mut endpoint_sockets = [relay_socket_index, other_socket.index];
                            if endpoints[0] > endpoints[1] {
                                endpoints.swap(0, 1);
                                endpoint_sockets.swap(0, 1);
                            }
                            let crossed_tiles = crossed_tiles(relay_socket, other_socket.position);
                            let passes_through_foreground = connection_passes_through_foreground(
                                relay.object_type,
                                other.object_type,
                            );
                            let candidate = CandidateConnection {
                                endpoints,
                                endpoint_sockets,
                                socket_start: relay_socket,
                                socket_end: other_socket.position,
                                visible: passes_through_foreground
                                    || has_line_of_sight(world, &crossed_tiles),
                                connected: false,
                                crossed_tiles,
                                passes_through_foreground,
                            };
                            let involves_cable = relay.object_type == POWERED_CABLE_OBJECT
                                || other.object_type == POWERED_CABLE_OBJECT;
                            if seen.insert(endpoints) {
                                let candidate_index = self.candidates.len();
                                self.candidates.push(candidate);
                                if involves_cable {
                                    cable_candidates.insert(endpoints, candidate_index);
                                }
                            } else if let Some(&candidate_index) = cable_candidates.get(&endpoints)
                            {
                                let current = &self.candidates[candidate_index];
                                if squared_socket_distance(
                                    candidate.socket_start,
                                    candidate.socket_end,
                                ) < squared_socket_distance(
                                    current.socket_start,
                                    current.socket_end,
                                ) {
                                    self.candidates[candidate_index] = candidate;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.candidates
            .sort_unstable_by_key(|candidate| candidate.endpoints);
        self.rebuild_candidate_indices();
    }

    pub(super) fn apply_topology_changes(
        &mut self,
        world: &World,
        changes: &[(ObjectId, ObjectTypeId)],
    ) -> bool {
        if changes.is_empty()
            || changes.iter().any(|&(_, object_type)| {
                object_type == POWERED_CABLE_OBJECT
                    || object_type == FurnitureObject::POWERED_CABLE_ANCHOR
                    || crate::furniture_definition(object_type).is_some_and(|definition| {
                        matches!(definition.behavior(), crate::FurnitureBehavior::CargoLift)
                    })
            })
        {
            return false;
        }

        for &(object, _) in changes {
            if world.object(object).is_some() {
                if self.node_indices.contains_key(&object) {
                    continue;
                }
                let Some(node) = power_node(world, object) else {
                    return false;
                };
                self.insert_node(node);
                self.insert_candidates_for_node(world, object);
            } else {
                self.remove_node(object);
            }
        }
        true
    }

    fn insert_node(&mut self, node: PowerNode) {
        let index = self.nodes.len();
        self.nodes.push(node);
        self.node_indices.insert(node.object, index);
        for (socket_index, socket) in node.sockets() {
            self.node_buckets
                .entry(socket_bucket(socket))
                .or_default()
                .push(PowerSocketRef {
                    object: node.object,
                    index: socket_index,
                    position: socket,
                });
        }
    }

    fn remove_node(&mut self, object: ObjectId) -> bool {
        let Some(index) = self.node_indices.remove(&object) else {
            return false;
        };
        let removed = self.nodes.swap_remove(index);
        if index < self.nodes.len() {
            self.node_indices.insert(self.nodes[index].object, index);
        }
        for (_, socket) in removed.sockets() {
            let bucket = socket_bucket(socket);
            if let Some(objects) = self.node_buckets.get_mut(&bucket) {
                objects.retain(|candidate| candidate.object != object);
                if objects.is_empty() {
                    self.node_buckets.remove(&bucket);
                }
            }
        }
        let mut candidate_index = 0;
        while candidate_index < self.candidates.len() {
            if self.candidates[candidate_index].endpoints.contains(&object) {
                self.swap_remove_candidate(candidate_index);
            } else {
                candidate_index += 1;
            }
        }
        true
    }

    fn insert_candidates_for_node(&mut self, world: &World, object: ObjectId) {
        let node = self.nodes[self.node_indices[&object]];
        let mut neighbours = Vec::<PowerSocketRef>::new();
        for (_, socket) in node.sockets() {
            let bucket = socket_bucket(socket);
            for bucket_y in bucket[1] - 1..=bucket[1] + 1 {
                for bucket_x in bucket[0] - 1..=bucket[0] + 1 {
                    if let Some(objects) = self.node_buckets.get(&[bucket_x, bucket_y]) {
                        neighbours.extend(objects.iter().copied());
                    }
                }
            }
        }
        let mut best_by_object = HashMap::<ObjectId, (u8, PowerSocketRef)>::new();
        for (node_socket_index, node_socket) in node.sockets() {
            for other_socket in neighbours.iter().copied() {
                let other_id = other_socket.object;
                if other_id == object {
                    continue;
                }
                let other = self.nodes[self.node_indices[&other_id]];
                if node.role != PowerRole::Relay && other.role != PowerRole::Relay {
                    continue;
                }
                if !automatic_connection_allowed(world, object, other_id)
                    || squared_socket_distance(node_socket, other_socket.position)
                        > connection_range_squared(&node, &other)
                {
                    continue;
                }
                let replace = best_by_object.get(&other_id).is_none_or(
                    |&(best_node_socket, best_other_socket)| {
                        squared_socket_distance(node_socket, other_socket.position)
                            < squared_socket_distance(
                                node.sockets[usize::from(best_node_socket)],
                                best_other_socket.position,
                            )
                    },
                );
                if replace {
                    best_by_object.insert(other_id, (node_socket_index, other_socket));
                }
            }
        }
        for (other_id, (node_socket_index, other_socket)) in best_by_object {
            let other = self.nodes[self.node_indices[&other_id]];
            let mut endpoints = [object, other_id];
            let mut endpoint_sockets = [node_socket_index, other_socket.index];
            if endpoints[0] > endpoints[1] {
                endpoints.swap(0, 1);
                endpoint_sockets.swap(0, 1);
            }
            let node_socket = node.sockets[usize::from(node_socket_index)];
            let crossed_tiles = crossed_tiles(node_socket, other_socket.position);
            let passes_through_foreground =
                connection_passes_through_foreground(node.object_type, other.object_type);
            let candidate_index = self.candidates.len();
            self.candidates.push(CandidateConnection {
                endpoints,
                endpoint_sockets,
                socket_start: node_socket,
                socket_end: other_socket.position,
                visible: passes_through_foreground || has_line_of_sight(world, &crossed_tiles),
                connected: false,
                crossed_tiles,
                passes_through_foreground,
            });
            if !passes_through_foreground {
                for &tile in &self.candidates[candidate_index].crossed_tiles {
                    self.candidates_by_tile
                        .entry(tile)
                        .or_default()
                        .push(candidate_index);
                }
            }
            self.dirty_candidate_marks.push(false);
        }
    }

    fn swap_remove_candidate(&mut self, index: usize) {
        let previous_last = self.candidates.len() - 1;
        let removed = self.candidates.swap_remove(index);
        self.dirty_candidate_marks.swap_remove(index);
        for tile in removed.crossed_tiles {
            if let Some(candidates) = self.candidates_by_tile.get_mut(&tile) {
                candidates.retain(|&candidate| candidate != index);
            }
        }
        if index == previous_last {
            return;
        }
        for &tile in &self.candidates[index].crossed_tiles {
            if let Some(candidates) = self.candidates_by_tile.get_mut(&tile)
                && let Some(candidate) = candidates
                    .iter_mut()
                    .find(|candidate| **candidate == previous_last)
            {
                *candidate = index;
            }
        }
    }

    fn rebuild_candidate_indices(&mut self) {
        self.candidates_by_tile.clear();
        for (index, candidate) in self.candidates.iter().enumerate() {
            if candidate.passes_through_foreground {
                continue;
            }
            for &tile in &candidate.crossed_tiles {
                self.candidates_by_tile.entry(tile).or_default().push(index);
            }
        }
        self.dirty_candidates.clear();
        self.dirty_candidate_marks.clear();
        self.dirty_candidate_marks
            .resize(self.candidates.len(), false);
    }

    pub(super) fn collect_dirty_candidates(&mut self, world: &World) {
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

    pub(super) fn rebuild_components(&mut self) {
        let mut union = UnionFind::new(self.nodes.len());
        let mut connected = vec![false; self.nodes.len()];
        let mut degrees = vec![0_usize; self.nodes.len()];
        let mut socket_degrees = vec![[0_usize; MAX_NODE_SOCKETS]; self.nodes.len()];
        for &[left_id, right_id] in &self.wired_edges {
            let (Some(&left), Some(&right)) = (
                self.node_indices.get(&left_id),
                self.node_indices.get(&right_id),
            ) else {
                continue;
            };
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
        for &candidate_index in &candidate_order {
            let [left_id, right_id] = self.candidates[candidate_index].endpoints;
            let (Some(&left), Some(&right)) = (
                self.node_indices.get(&left_id),
                self.node_indices.get(&right_id),
            ) else {
                continue;
            };
            let [left_socket, right_socket] = self.candidates[candidate_index].endpoint_sockets;
            if degrees[left] >= self.nodes[left].connection_limit
                || degrees[right] >= self.nodes[right].connection_limit
                || socket_degrees[left][usize::from(left_socket)]
                    >= self.nodes[left].socket_connection_limit
                || socket_degrees[right][usize::from(right_socket)]
                    >= self.nodes[right].socket_connection_limit
            {
                continue;
            }
            self.candidates[candidate_index].connected = true;
            degrees[left] += 1;
            degrees[right] += 1;
            socket_degrees[left][usize::from(left_socket)] += 1;
            socket_degrees[right][usize::from(right_socket)] += 1;
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

        candidate_order.retain(|&index| self.candidates[index].connected);
        candidate_order.sort_unstable_by_key(|&index| self.candidates[index].endpoints);
        self.connections.clear();
        self.connections
            .extend(candidate_order.iter().map(|&index| {
                let candidate = &self.candidates[index];
                PowerConnection {
                    endpoints: candidate.endpoints,
                    start: socket_position(candidate.socket_start),
                    end: socket_position(candidate.socket_end),
                }
            }));

        self.connections_by_chunk.clear();
        for (connection_index, &candidate_index) in candidate_order.iter().enumerate() {
            let candidate = &self.candidates[candidate_index];
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

fn connection_range_squared(left: &PowerNode, right: &PowerNode) -> i64 {
    match (left.object_type, right.object_type) {
        (FurnitureObject::PYLON, FurnitureObject::POWER_CONNECTOR) => left.connection_range_squared,
        (FurnitureObject::POWER_CONNECTOR, FurnitureObject::PYLON) => {
            right.connection_range_squared
        }
        _ => left
            .connection_range_squared
            .min(right.connection_range_squared),
    }
}

fn connection_passes_through_foreground(left: ObjectTypeId, right: ObjectTypeId) -> bool {
    left == FurnitureObject::POWER_CONNECTOR || right == FurnitureObject::POWER_CONNECTOR
}

fn power_node(world: &World, object: ObjectId) -> Option<PowerNode> {
    let object = world.object(object)?;
    let definition = crate::furniture_definition(object.object_type())?;
    let role = definition.power_role()?;
    let offset = definition.power_socket_half_tiles()?;
    let connection_limit = match role {
        PowerRole::Relay => usize::from(definition.power_connection_limit()),
        PowerRole::Generator | PowerRole::Consumer | PowerRole::Storage => MACHINE_CONNECTION_LIMIT,
    };
    Some(PowerNode {
        object: object.id(),
        object_type: object.object_type(),
        role,
        sockets: [
            [
                i64::from(object.anchor().x) * SOCKET_UNITS_PER_TILE + i64::from(offset[0]),
                i64::from(object.anchor().y) * SOCKET_UNITS_PER_TILE + i64::from(offset[1]),
            ],
            [0; 2],
        ],
        socket_count: 1,
        rate_milli_per_second: definition.power_rate_milli_per_second(),
        capacity_milli: definition.power_capacity_milli(),
        connection_limit,
        socket_connection_limit: connection_limit,
        connection_range_squared: match role {
            PowerRole::Relay => {
                let range = i64::from(definition.power_connection_range_half_tiles());
                range * range
            }
            PowerRole::Generator | PowerRole::Consumer | PowerRole::Storage => {
                CONNECTION_RANGE_SQUARED
            }
        },
    })
}

fn powered_cable_node(object: &crate::WorldObject) -> PowerNode {
    let anchor = object.anchor();
    let bottom_y = anchor.y + u32::from(object.size()[1]) - 1;
    let top = [
        i64::from(anchor.x) * SOCKET_UNITS_PER_TILE,
        i64::from(anchor.y) * SOCKET_UNITS_PER_TILE,
    ];
    let bottom = [
        i64::from(anchor.x) * SOCKET_UNITS_PER_TILE,
        i64::from(bottom_y) * SOCKET_UNITS_PER_TILE,
    ];
    PowerNode {
        object: object.id(),
        object_type: POWERED_CABLE_OBJECT,
        role: PowerRole::Relay,
        sockets: [top, bottom],
        socket_count: if top == bottom { 1 } else { 2 },
        rate_milli_per_second: 0,
        capacity_milli: 0,
        connection_limit: POWERED_CABLE_CONNECTION_LIMIT,
        socket_connection_limit: POWERED_CABLE_ENDPOINT_CONNECTION_LIMIT,
        connection_range_squared: CONNECTION_RANGE_SQUARED,
    }
}
