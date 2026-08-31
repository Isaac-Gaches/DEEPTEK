use super::*;
use crate::{CargoLiftDirection, FurnitureDefinition, Layer, POWERED_CABLE_OBJECT, ROPE_OBJECT};

pub(super) fn encode_objects(world: &World) -> Vec<u8> {
    let container_bytes: usize = world
        .objects
        .containers
        .values()
        .map(|container| CONTAINER_HEADER_SIZE + container.slots().len() * CONTAINER_SLOT_SIZE)
        .sum();
    let mut output = Vec::with_capacity(
        OBJECT_HEADER_SIZE + world.objects.objects.len() * OBJECT_RECORD_SIZE + container_bytes,
    );
    output.extend_from_slice(&world.simulation_tick.to_le_bytes());
    output.extend_from_slice(&world.simulation_remainder_nanos.to_le_bytes());
    output.extend_from_slice(&world.objects.next_id.to_le_bytes());
    output.extend_from_slice(&(world.objects.objects.len() as u32).to_le_bytes());
    output.extend_from_slice(&(world.objects.containers.len() as u32).to_le_bytes());
    for object in &world.objects.objects {
        output.extend_from_slice(&object.id.raw().to_le_bytes());
        output.extend_from_slice(&object.object_type.raw().to_le_bytes());
        output.push(object.variant);
        output.push(object.growth_stage);
        output.push(u8::from(object.active));
        output.extend_from_slice(&object.anchor.x.to_le_bytes());
        output.extend_from_slice(&object.anchor.y.to_le_bytes());
        output.extend_from_slice(&object.root.x.to_le_bytes());
        output.extend_from_slice(&object.root.y.to_le_bytes());
        output.extend_from_slice(&object.width.to_le_bytes());
        output.extend_from_slice(&object.height.to_le_bytes());
        output.extend_from_slice(&object.next_update_tick.to_le_bytes());
        output.extend_from_slice(&object.stored_energy_milli.to_le_bytes());
        output.extend_from_slice(&object.machine_target_y.to_le_bytes());
        output.extend_from_slice(&object.kill_count.to_le_bytes());
        output.extend_from_slice(&object.linked_object.to_le_bytes());
        output.extend_from_slice(&object.motion_position_milli.to_le_bytes());
        output.extend_from_slice(&object.health.to_le_bytes());
    }
    let mut containers: Vec<_> = world.objects.containers.iter().collect();
    containers.sort_unstable_by_key(|(id, _)| **id);
    for (id, container) in containers {
        output.extend_from_slice(&id.raw().to_le_bytes());
        output.extend_from_slice(&(container.slots().len() as u16).to_le_bytes());
        output.extend_from_slice(&0_u16.to_le_bytes());
        for slot in container.slots() {
            let (item, quantity) = slot
                .map(|stack| (stack.item().raw(), stack.quantity()))
                .unwrap_or((0, 0));
            output.extend_from_slice(&item.to_le_bytes());
            output.extend_from_slice(&quantity.to_le_bytes());
        }
    }
    output
}

pub(super) fn decode_objects(
    world: &mut World,
    bytes: &[u8],
    version: u16,
) -> Result<(), WorldError> {
    if bytes.len() < OBJECT_HEADER_SIZE {
        return invalid("object section is shorter than its header");
    }
    let mut cursor = 0;
    let simulation_tick = read_u64(bytes, &mut cursor)?;
    let simulation_remainder_nanos = read_u64(bytes, &mut cursor)?;
    if simulation_remainder_nanos >= 1_000_000_000 {
        return invalid("simulation remainder is outside one tick");
    }
    let next_id = read_u64(bytes, &mut cursor)?;
    let object_count = read_u32(bytes, &mut cursor)? as usize;
    let container_count = read_u32(bytes, &mut cursor)? as usize;
    if version == OBJECTS_VERSION && container_count != 0 {
        return invalid("version 2 object section reserved field is non-zero");
    }
    let object_record_size = if version >= MACHINE_HEALTH_VERSION {
        OBJECT_RECORD_SIZE
    } else if version >= MOTION_VERSION {
        MOTION_OBJECT_RECORD_SIZE
    } else if version >= STATISTICS_VERSION {
        STATISTICS_OBJECT_RECORD_SIZE
    } else if version >= BATTERY_STORAGE_VERSION {
        BATTERY_OBJECT_RECORD_SIZE
    } else if version >= ACTIVATION_VERSION {
        ACTIVATION_OBJECT_RECORD_SIZE
    } else {
        LEGACY_OBJECT_RECORD_SIZE
    };
    let object_records_len = object_count
        .checked_mul(object_record_size)
        .ok_or_else(|| WorldError::InvalidData("object count overflow".into()))?;
    if bytes.len().saturating_sub(cursor) < object_records_len {
        return invalid("object count exceeds the object section length");
    }
    if object_count > world.width as usize * world.height as usize {
        return invalid("object count exceeds the number of world cells");
    }

    let mut store = ObjectStore::new(world.chunks.len(), world.chunks_wide);
    let mut max_id = 0_u64;
    for _ in 0..object_count {
        let id = ObjectId::from_raw(read_u64(bytes, &mut cursor)?);
        if id.raw() == 0 || store.object(id).is_some() {
            return invalid("object IDs must be non-zero and unique");
        }
        max_id = max_id.max(id.raw());
        let object_type = ObjectTypeId::new(read_u16(bytes, &mut cursor)?);
        let variant = take(bytes, &mut cursor, 1)?[0];
        let growth_stage = take(bytes, &mut cursor, 1)?[0];
        let active = if version >= ACTIVATION_VERSION {
            match take(bytes, &mut cursor, 1)?[0] {
                0 => false,
                1 => true,
                _ => return invalid("object active flag is not boolean"),
            }
        } else {
            object_type != FurnitureObject::LASER_BORE
        };
        let anchor = TilePos {
            x: read_u32(bytes, &mut cursor)?,
            y: read_u32(bytes, &mut cursor)?,
        };
        let mut root = TilePos {
            x: read_u32(bytes, &mut cursor)?,
            y: read_u32(bytes, &mut cursor)?,
        };
        let width = read_u16(bytes, &mut cursor)?;
        let height = read_u16(bytes, &mut cursor)?;
        let mut next_update_tick = read_u64(bytes, &mut cursor)?;
        let stored_energy_milli = if version >= BATTERY_STORAGE_VERSION {
            read_u32(bytes, &mut cursor)?
        } else {
            0
        };
        let machine_target_y = if version >= BATTERY_STORAGE_VERSION {
            read_u32(bytes, &mut cursor)?
        } else {
            u32::MAX
        };
        let kill_count = if version >= STATISTICS_VERSION {
            read_u32(bytes, &mut cursor)?
        } else {
            0
        };
        let linked_object = if version >= MOTION_VERSION {
            read_u64(bytes, &mut cursor)?
        } else {
            0
        };
        let motion_position_milli = if version >= MOTION_VERSION {
            read_u32(bytes, &mut cursor)?
        } else {
            0
        };
        let health = if version >= MACHINE_HEALTH_VERSION {
            read_u16(bytes, &mut cursor)?
        } else {
            furniture_definition(object_type)
                .and_then(|definition| definition.maximum_health())
                .unwrap_or(0)
        };
        if !active
            && furniture_definition(object_type)
                .is_some_and(|definition| definition.interaction().is_activatable())
        {
            next_update_tick = u64::MAX;
        }
        if machine_target_y != u32::MAX
            && (!matches!(
                object_type,
                FurnitureObject::LASER_BORE
                    | FurnitureObject::RED_SHAFT_BORE
                    | FurnitureObject::LASER_DRILL
            ) || machine_target_y >= world.height)
        {
            return invalid("machine target is invalid");
        }
        if width == 0 || height == 0 {
            return invalid("object dimensions must be non-zero");
        }
        let end_x = anchor
            .x
            .checked_add(u32::from(width) - 1)
            .ok_or_else(|| WorldError::InvalidData("object width overflow".into()))?;
        let end_y = anchor
            .y
            .checked_add(u32::from(height) - 1)
            .ok_or_else(|| WorldError::InvalidData("object height overflow".into()))?;
        if end_x >= world.width
            || end_y >= world.height
            || root.x >= world.width
            || root.y >= world.height
        {
            return invalid("object footprint or root is outside the world");
        }
        for y in anchor.y..=end_y {
            for x in anchor.x..=end_x {
                let furniture = furniture_definition(object_type);
                if world.tile_in_bounds(x, y, Layer::Foreground) != TileId::EMPTY
                    && (furniture.is_none()
                        || furniture.is_some_and(FurnitureDefinition::is_structural))
                    && !matches!(object_type, ROPE_OBJECT | POWERED_CABLE_OBJECT)
                {
                    return invalid("object footprint overlaps a foreground tile");
                }
            }
        }
        if let Some(definition) = furniture_definition(object_type) {
            if [width, height] != definition.size() {
                return invalid("furniture dimensions do not match its definition");
            }
            if definition.interaction().configuration()
                == Some(FurnitureConfiguration::TargetPriority)
                && TargetPriority::from_raw(configuration_variant(variant)).is_none()
            {
                return invalid("furniture target priority is invalid");
            }
            if definition.interaction().configuration() == Some(FurnitureConfiguration::LaserAim)
                && LaserDrillAim::from_raw(variant).is_none()
            {
                return invalid("laser drill aim is invalid");
            }
            if definition.supports_facing()
                && definition.interaction().configuration().is_none()
                && configuration_variant(variant) != 0
            {
                return invalid("directional furniture facing state is invalid");
            }
            match definition.support() {
                FurnitureSupport::Floor | FurnitureSupport::FloorEdges => {
                    let expected_root_y = end_y
                        .checked_add(1)
                        .ok_or_else(|| WorldError::InvalidData("furniture root overflow".into()))?;
                    if root != TilePos::new(anchor.x, expected_root_y) {
                        return invalid("furniture root does not match its floor support row");
                    }
                    if expected_root_y >= world.height {
                        return invalid("furniture floor support is outside the world");
                    }
                    for column in 0..width {
                        if !definition.support().requires_column(column, width) {
                            continue;
                        }
                        let x = anchor.x + u32::from(column);
                        if !decoded_cell_is_solid(world, &store, TilePos::new(x, expected_root_y)) {
                            return invalid("furniture floor support tile is empty");
                        }
                    }
                }
                FurnitureSupport::Side => {
                    let valid_root = if root == anchor {
                        world.tile_in_bounds(anchor.x, anchor.y, Layer::Background) != TileId::EMPTY
                    } else {
                        let dx = root.x.abs_diff(anchor.x);
                        let dy = root.y.abs_diff(anchor.y);
                        dx + dy == 1 && decoded_cell_is_solid(world, &store, root)
                    };
                    if !valid_root {
                        return invalid("side-supported furniture root is missing");
                    }
                }
                FurnitureSupport::Free => {
                    let legacy_floor_root = TilePos::new(anchor.x, end_y.saturating_add(1));
                    if root == legacy_floor_root && definition.is_item_transport_connector() {
                        root = anchor;
                    } else if root != anchor {
                        return invalid("free-standing furniture root does not match its anchor");
                    }
                }
            }
            if stored_energy_milli > definition.power_capacity_milli() {
                return invalid("stored energy exceeds furniture capacity");
            }
            match definition.maximum_health() {
                Some(maximum) if health > maximum => {
                    return invalid("machine health exceeds its definition maximum");
                }
                None if health != 0 => return invalid("passive furniture contains machine health"),
                Some(_) | None => {}
            }
            if kill_count != 0 && !crate::terrain::world_objects::is_turret_type(object_type) {
                return invalid("non-turret furniture contains a kill count");
            }
            if object_type == FurnitureObject::CARGO_LIFT {
                if linked_object == 0
                    || CargoLiftDirection::from_raw(variant).is_none()
                    || motion_position_milli / 1_000 >= world.height
                    || motion_position_milli.saturating_add(500) / 1_000 != anchor.y
                {
                    return invalid("cargo lift motion state is invalid");
                }
            } else if object_type == FurnitureObject::LIFT_STATION {
                if version < LIFT_STATION_VERSION
                    || linked_object == 0
                    || motion_position_milli != 0
                    || LiftStationConfiguration::from_raw(variant).is_none()
                {
                    return invalid("lift station state is invalid");
                }
            } else if linked_object != 0 || motion_position_milli != 0 {
                return invalid("stationary furniture contains cargo lift motion state");
            }
        } else if stored_energy_milli != 0 {
            return invalid("non-furniture object contains stored energy");
        } else if kill_count != 0 {
            return invalid("non-furniture object contains a kill count");
        } else if linked_object != 0 || motion_position_milli != 0 {
            return invalid("non-furniture object contains cargo lift motion state");
        } else if object_type == ROPE_OBJECT
            && world.tile_in_bounds(root.x, root.y, Layer::Foreground) == TileId::EMPTY
            && (root != anchor
                || world.tile_in_bounds(root.x, root.y, Layer::Background) == TileId::EMPTY)
        {
            return invalid(format!(
                "object {} type {} root ({}, {}) is empty",
                id.raw(),
                object_type.raw(),
                root.x,
                root.y
            ));
        } else if !matches!(object_type, ROPE_OBJECT | POWERED_CABLE_OBJECT)
            && world.tile_in_bounds(root.x, root.y, Layer::Foreground) == TileId::EMPTY
        {
            return invalid("object root tile is empty");
        }
        store
            .insert(WorldObject {
                id,
                object_type,
                anchor,
                root,
                width,
                height,
                variant,
                growth_stage,
                active,
                health,
                stored_energy_milli,
                machine_target_y,
                kill_count,
                linked_object,
                motion_position_milli,
                next_update_tick,
            })
            .map_err(|error| WorldError::InvalidData(error.to_string()))?;
    }
    let lift_sides: HashMap<_, _> = store
        .objects
        .iter()
        .filter(|object| object.object_type == FurnitureObject::CARGO_LIFT)
        .filter_map(|lift| {
            store
                .object(ObjectId::from_raw(lift.linked_object))
                .map(|cable| (cable.id, lift.anchor.x > cable.anchor.x))
        })
        .collect();
    let mut station_sides = HashMap::<ObjectId, bool>::new();
    let mut station_heights = HashSet::<(ObjectId, u32)>::new();
    for object in &store.objects {
        if object.object_type == POWERED_CABLE_OBJECT {
            let background_support = object.root == object.anchor
                && world.tile_in_bounds(object.anchor.x, object.anchor.y, Layer::Background)
                    != TileId::EMPTY;
            let adjacent_support = object.root.x.abs_diff(object.anchor.x)
                + object.root.y.abs_diff(object.anchor.y)
                == 1
                && (decoded_cell_is_solid(world, &store, object.root)
                    || store
                        .occupying(object.root)
                        .and_then(|anchor| store.object(anchor))
                        .is_some_and(|anchor| {
                            anchor.object_type == FurnitureObject::POWERED_CABLE_ANCHOR
                        }));
            if object.width != 1 || !background_support && !adjacent_support {
                return invalid("powered cable is missing top support");
            }
        }
        if object.object_type == FurnitureObject::CARGO_LIFT {
            let cable = store
                .object(ObjectId::from_raw(object.linked_object))
                .filter(|cable| cable.object_type == POWERED_CABLE_OBJECT)
                .ok_or_else(|| {
                    WorldError::InvalidData("cargo lift references a missing powered cable".into())
                })?;
            let maximum = cable
                .anchor
                .y
                .checked_add(u32::from(cable.height).saturating_sub(2));
            if cable.height < 2
                || maximum.is_none()
                || {
                    let maximum = maximum.unwrap();
                    !(cable.anchor.y..=maximum).contains(&object.anchor.y)
                }
                || !(object.anchor.x == cable.anchor.x + 1
                    || object.anchor.x.checked_add(2) == Some(cable.anchor.x))
            {
                return invalid("cargo lift is outside its powered cable track");
            }
        }
        if object.object_type == FurnitureObject::LIFT_STATION {
            let cable = store
                .object(ObjectId::from_raw(object.linked_object))
                .filter(|cable| cable.object_type == POWERED_CABLE_OBJECT)
                .ok_or_else(|| {
                    WorldError::InvalidData(
                        "lift station references a missing powered cable".into(),
                    )
                })?;
            let maximum = cable
                .anchor
                .y
                .checked_add(u32::from(cable.height).saturating_sub(2));
            let is_right = object.anchor.x == cable.anchor.x + 1;
            let is_left = object.anchor.x.checked_add(2) == Some(cable.anchor.x);
            if cable.height < 2
                || maximum.is_none()
                || !(cable.anchor.y..=maximum.unwrap()).contains(&object.anchor.y)
                || !is_right && !is_left
                || lift_sides
                    .get(&cable.id)
                    .is_some_and(|&lift_is_right| lift_is_right == is_right)
                || station_sides
                    .insert(cable.id, is_right)
                    .is_some_and(|side| side != is_right)
                || !station_heights.insert((cable.id, object.anchor.y))
            {
                return invalid("lift station is outside its powered cable track");
            }
        }
    }
    if version == OBJECTS_VERSION {
        if cursor != bytes.len() {
            return invalid("version 2 object count does not match section length");
        }
        for object in &store.objects {
            if let Some(slots) = furniture_definition(object.object_type)
                .and_then(|definition| definition.interaction().container_slots())
            {
                store
                    .containers
                    .insert(object.id, ItemContainer::new(usize::from(slots)));
            }
        }
    } else {
        decode_containers(&mut store, bytes, &mut cursor, container_count, version)?;
        if cursor != bytes.len() {
            return invalid("unexpected trailing container data");
        }
    }
    if next_id <= max_id {
        return invalid("next object ID does not follow saved object IDs");
    }
    store.next_id = next_id;
    world.objects = store;
    world.simulation_tick = simulation_tick;
    world.simulation_remainder_nanos = simulation_remainder_nanos;
    Ok(())
}

fn decoded_cell_is_solid(world: &World, store: &ObjectStore, position: TilePos) -> bool {
    world.tile_in_bounds(position.x, position.y, Layer::Foreground) != TileId::EMPTY
        || store.occupying(position).is_some_and(|id| {
            store.object(id).is_some_and(|object| {
                furniture_definition(object.object_type)
                    .is_some_and(FurnitureDefinition::is_structural)
            })
        })
}

pub(super) fn decode_containers(
    store: &mut ObjectStore,
    bytes: &[u8],
    cursor: &mut usize,
    container_count: usize,
    version: u16,
) -> Result<(), WorldError> {
    if container_count > store.objects.len() {
        return invalid("container count exceeds object count");
    }
    for _ in 0..container_count {
        let object_id = ObjectId::from_raw(read_u64(bytes, cursor)?);
        let slot_count = read_u16(bytes, cursor)?;
        let reserved = read_u16(bytes, cursor)?;
        if reserved != 0 {
            return invalid("container reserved field is non-zero");
        }
        if store.containers.contains_key(&object_id) {
            return invalid("container object IDs must be unique");
        }
        let object = store.object(object_id).ok_or_else(|| {
            WorldError::InvalidData("container references a missing object".into())
        })?;
        let expected_slots = furniture_definition(object.object_type)
            .and_then(|definition| definition.interaction().container_slots())
            .ok_or_else(|| {
                WorldError::InvalidData("container references non-container furniture".into())
            })?;
        if slot_count != expected_slots {
            return invalid("container slot count does not match its furniture definition");
        }
        let mut container = ItemContainer::new(usize::from(slot_count));
        for slot in 0..usize::from(slot_count) {
            let item = read_u16(bytes, cursor)?;
            let quantity = read_u16(bytes, cursor)?;
            let stack = match (item, quantity) {
                (0, 0) => None,
                (0, _) | (_, 0) => return invalid("container slot has a partial empty stack"),
                (item, quantity) => ItemStack::new(ItemId::new(item), quantity),
            };
            debug_assert!(container.set_slot(slot, stack));
        }
        store.containers.insert(object_id, container);
    }
    if version < BORE_CONTAINER_VERSION {
        let legacy_bores: Vec<_> = store
            .objects
            .iter()
            .filter(|object| object.object_type == FurnitureObject::LASER_BORE)
            .filter(|object| !store.containers.contains_key(&object.id))
            .map(|object| object.id)
            .collect();
        for id in legacy_bores {
            let slots = furniture_definition(FurnitureObject::LASER_BORE)
                .and_then(|definition| definition.interaction().container_slots())
                .expect("laser bores expose container storage");
            store
                .containers
                .insert(id, ItemContainer::new(usize::from(slots)));
        }
    }
    let missing_container = store.objects.iter().any(|object| {
        furniture_definition(object.object_type).is_some_and(|definition| {
            definition.interaction().container_slots().is_some()
                && !store.containers.contains_key(&object.id)
        })
    });
    if missing_container {
        return invalid("container furniture is missing its storage record");
    }
    Ok(())
}
