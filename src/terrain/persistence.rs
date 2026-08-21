use super::{
    CHUNK_AREA, CHUNK_SIZE, FurnitureConfiguration, FurnitureObject, FurnitureSupport,
    LiftStationConfiguration, MAX_WORLD_NAME_BYTES, ObjectId, ObjectTypeId, TargetPriority, TileId,
    TilePos, World, WorldError, WorldObject, furniture_definition, objects::ObjectStore,
    parallel_mut,
};
use crate::items::{ItemContainer, ItemId, ItemStack};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 8] = b"DTKWLD\0\0";
const VERSION: u16 = 11;
const MOTION_VERSION: u16 = 10;
const STATISTICS_VERSION: u16 = 9;
const BATTERY_STORAGE_VERSION: u16 = 8;
const ACTIVATION_VERSION: u16 = 7;
const BORE_CONTAINER_VERSION: u16 = 6;
const SESSION_METADATA_VERSION: u16 = 5;
const NAMED_WORLD_VERSION: u16 = 4;
const CONTAINERS_VERSION: u16 = 3;
const OBJECTS_VERSION: u16 = 2;
const LEGACY_VERSION: u16 = 1;
const HEADER_SIZE: usize = 36;
const SESSION_METADATA_SIZE: usize = 16;
const RECORD_SIZE: usize = 16;
const OBJECT_HEADER_SIZE: usize = 32;
const LEGACY_OBJECT_RECORD_SIZE: usize = 40;
const ACTIVATION_OBJECT_RECORD_SIZE: usize = 41;
const BATTERY_OBJECT_RECORD_SIZE: usize = 49;
const STATISTICS_OBJECT_RECORD_SIZE: usize = 53;
const OBJECT_RECORD_SIZE: usize = 65;
const CONTAINER_HEADER_SIZE: usize = 12;
const CONTAINER_SLOT_SIZE: usize = 4;

struct EncodedChunk {
    foreground: Vec<u8>,
    background: Vec<u8>,
}

#[derive(Clone)]
struct ChunkRecord {
    foreground: Range<usize>,
    background: Range<usize>,
    foreground_checksum: u32,
    background_checksum: u32,
}

pub(super) fn save(world: &World, path: &Path, threads: usize) -> Result<(), WorldError> {
    if world.name.len() > MAX_WORLD_NAME_BYTES {
        return invalid(format!(
            "world name exceeds {MAX_WORLD_NAME_BYTES} UTF-8 bytes"
        ));
    }
    validate_container_store(world)?;
    let mut encoded: Vec<_> = (0..world.chunks.len())
        .map(|_| EncodedChunk {
            foreground: Vec::new(),
            background: Vec::new(),
        })
        .collect();
    parallel_mut(&mut encoded, threads, |index, output| {
        output.foreground = encode_layer(&world.chunks[index].foreground);
        output.background = encode_layer(&world.chunks[index].background);
    })?;
    let object_payload = encode_objects(world);

    let temporary = sibling_path(path, "tmp");
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(MAGIC)?;
        file.write_all(&VERSION.to_le_bytes())?;
        file.write_all(&(CHUNK_SIZE as u16).to_le_bytes())?;
        file.write_all(&world.width.to_le_bytes())?;
        file.write_all(&world.height.to_le_bytes())?;
        file.write_all(&world.seed.to_le_bytes())?;
        file.write_all(&(world.chunks.len() as u32).to_le_bytes())?;
        file.write_all(&(world.name.len() as u32).to_le_bytes())?;
        file.write_all(world.name.as_bytes())?;
        file.write_all(&world.time_of_day().to_bits().to_le_bytes())?;
        if let Some([x, y]) = world.player_position() {
            file.write_all(&1_u32.to_le_bytes())?;
            file.write_all(&x.to_bits().to_le_bytes())?;
            file.write_all(&y.to_bits().to_le_bytes())?;
        } else {
            file.write_all(&[0; 12])?;
        }
        for chunk in &encoded {
            file.write_all(&(chunk.foreground.len() as u32).to_le_bytes())?;
            file.write_all(&(chunk.background.len() as u32).to_le_bytes())?;
            file.write_all(&checksum(&chunk.foreground).to_le_bytes())?;
            file.write_all(&checksum(&chunk.background).to_le_bytes())?;
            file.write_all(&chunk.foreground)?;
            file.write_all(&chunk.background)?;
        }
        file.write_all(&(object_payload.len() as u32).to_le_bytes())?;
        file.write_all(&checksum(&object_payload).to_le_bytes())?;
        file.write_all(&object_payload)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_container_store(world: &World) -> Result<(), WorldError> {
    for object in &world.objects.objects {
        let expected_slots = furniture_definition(object.object_type)
            .and_then(|definition| definition.interaction().container_slots());
        match (expected_slots, world.objects.containers.get(&object.id)) {
            (Some(slots), Some(container)) if container.slots().len() == usize::from(slots) => {}
            (Some(_), Some(_)) => {
                return invalid("container slot count does not match its furniture definition");
            }
            (Some(_), None) => return invalid("container furniture is missing its storage record"),
            (None, Some(_)) => return invalid("non-container object has a storage record"),
            (None, None) => {}
        }
    }
    for &id in world.objects.containers.keys() {
        if world.objects.object(id).is_none() {
            return invalid("container storage references a missing object");
        }
    }
    Ok(())
}

pub(super) fn load(path: &Path, threads: usize) -> Result<World, WorldError> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() < HEADER_SIZE {
        return invalid("file is shorter than its header");
    }
    let mut cursor = 0;
    if take(&bytes, &mut cursor, MAGIC.len())? != MAGIC {
        return invalid("incorrect file signature");
    }
    let version = read_u16(&bytes, &mut cursor)?;
    if !matches!(
        version,
        LEGACY_VERSION
            | OBJECTS_VERSION
            | CONTAINERS_VERSION
            | NAMED_WORLD_VERSION
            | SESSION_METADATA_VERSION
            | BORE_CONTAINER_VERSION
            | ACTIVATION_VERSION
            | BATTERY_STORAGE_VERSION
            | STATISTICS_VERSION
            | MOTION_VERSION
            | VERSION
    ) {
        return invalid(format!("unsupported version {version}"));
    }
    let chunk_size = read_u16(&bytes, &mut cursor)? as usize;
    if chunk_size != CHUNK_SIZE {
        return invalid(format!("unsupported chunk size {chunk_size}"));
    }
    let width = read_u32(&bytes, &mut cursor)?;
    let height = read_u32(&bytes, &mut cursor)?;
    let seed = read_u64(&bytes, &mut cursor)?;
    let chunk_count = read_u32(&bytes, &mut cursor)? as usize;
    let name_len = read_u32(&bytes, &mut cursor)? as usize;
    let name = if version >= NAMED_WORLD_VERSION {
        decode_name(take(&bytes, &mut cursor, name_len)?)?
    } else {
        if name_len != 0 {
            return invalid("legacy world header reserved field is non-zero");
        }
        String::new()
    };
    let session = if version >= SESSION_METADATA_VERSION {
        let session_bytes = take(&bytes, &mut cursor, SESSION_METADATA_SIZE)?;
        let mut session_cursor = 0;
        let time_of_day = f32::from_bits(read_u32(session_bytes, &mut session_cursor)?);
        let has_player_position = read_u32(session_bytes, &mut session_cursor)?;
        let player_x = f32::from_bits(read_u32(session_bytes, &mut session_cursor)?);
        let player_y = f32::from_bits(read_u32(session_bytes, &mut session_cursor)?);
        if has_player_position > 1 {
            return invalid("invalid player position flag");
        }
        Some((
            time_of_day,
            (has_player_position == 1).then_some([player_x, player_y]),
        ))
    } else {
        None
    };
    let mut world = World::empty(width, height, seed)?;
    world.name = name;
    if let Some((time_of_day, player_position)) = session {
        world.set_time_of_day(time_of_day)?;
        world.set_player_position(player_position)?;
    }
    if chunk_count != world.chunks.len() {
        return invalid("chunk count does not match world dimensions");
    }

    let mut records = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        if bytes.len().saturating_sub(cursor) < RECORD_SIZE {
            return invalid("truncated chunk record");
        }
        let foreground_len = read_u32(&bytes, &mut cursor)? as usize;
        let background_len = read_u32(&bytes, &mut cursor)? as usize;
        let foreground_checksum = read_u32(&bytes, &mut cursor)?;
        let background_checksum = read_u32(&bytes, &mut cursor)?;
        // A valid RLE plane needs at most one four-byte pair per tile.
        if foreground_len > CHUNK_AREA * 4 || background_len > CHUNK_AREA * 4 {
            return invalid("compressed tile plane exceeds its maximum size");
        }
        let foreground_start = cursor;
        let foreground_end = cursor
            .checked_add(foreground_len)
            .ok_or_else(|| WorldError::InvalidData("chunk length overflow".into()))?;
        let background_end = foreground_end
            .checked_add(background_len)
            .ok_or_else(|| WorldError::InvalidData("chunk length overflow".into()))?;
        if background_end > bytes.len() {
            return invalid("truncated chunk payload");
        }
        records.push(ChunkRecord {
            foreground: foreground_start..foreground_end,
            background: foreground_end..background_end,
            foreground_checksum,
            background_checksum,
        });
        cursor = background_end;
    }
    let object_payload = if version >= OBJECTS_VERSION {
        let payload_len = read_u32(&bytes, &mut cursor)? as usize;
        let expected_checksum = read_u32(&bytes, &mut cursor)?;
        let payload = take(&bytes, &mut cursor, payload_len)?;
        if checksum(payload) != expected_checksum {
            return invalid("object section checksum mismatch");
        }
        Some(payload)
    } else {
        None
    };
    if cursor != bytes.len() {
        return invalid("unexpected trailing data");
    }

    let failures = std::sync::Mutex::new(Vec::new());
    parallel_mut(&mut world.chunks, threads, |index, chunk| {
        let record = &records[index];
        let foreground = &bytes[record.foreground.clone()];
        let background = &bytes[record.background.clone()];
        let result = if checksum(foreground) != record.foreground_checksum
            || checksum(background) != record.background_checksum
        {
            Err("checksum mismatch".to_owned())
        } else {
            decode_layer(foreground, &mut chunk.foreground)
                .and_then(|()| decode_layer(background, &mut chunk.background))
        };
        if let Err(message) = result {
            failures.lock().unwrap().push((index, message));
        }
    })?;
    if let Some((index, message)) = failures.into_inner().unwrap().into_iter().next() {
        return invalid(format!("chunk {index}: {message}"));
    }
    if let Some(payload) = object_payload {
        decode_objects(&mut world, payload, version)?;
    }
    Ok(world)
}

pub(super) fn read_name(path: &Path) -> Result<Option<String>, WorldError> {
    let mut file = File::open(path)?;
    let mut header = [0_u8; HEADER_SIZE];
    file.read_exact(&mut header)?;
    let mut cursor = 0;
    if take(&header, &mut cursor, MAGIC.len())? != MAGIC {
        return invalid("incorrect file signature");
    }
    let version = read_u16(&header, &mut cursor)?;
    if !matches!(
        version,
        LEGACY_VERSION
            | OBJECTS_VERSION
            | CONTAINERS_VERSION
            | NAMED_WORLD_VERSION
            | SESSION_METADATA_VERSION
            | BORE_CONTAINER_VERSION
            | ACTIVATION_VERSION
            | BATTERY_STORAGE_VERSION
            | STATISTICS_VERSION
            | MOTION_VERSION
            | VERSION
    ) {
        return invalid(format!("unsupported version {version}"));
    }
    let chunk_size = read_u16(&header, &mut cursor)? as usize;
    if chunk_size != CHUNK_SIZE {
        return invalid(format!("unsupported chunk size {chunk_size}"));
    }
    cursor += 4 + 4 + 8 + 4;
    let name_len = read_u32(&header, &mut cursor)? as usize;
    if version < NAMED_WORLD_VERSION {
        if name_len != 0 {
            return invalid("legacy world header reserved field is non-zero");
        }
        return Ok(None);
    }
    if name_len > MAX_WORLD_NAME_BYTES {
        return invalid("world name exceeds its maximum encoded length");
    }
    let mut name = vec![0_u8; name_len];
    file.read_exact(&mut name)?;
    decode_name(&name).map(Some)
}

fn decode_name(bytes: &[u8]) -> Result<String, WorldError> {
    if bytes.len() > MAX_WORLD_NAME_BYTES {
        return invalid("world name exceeds its maximum encoded length");
    }
    let name = std::str::from_utf8(bytes)
        .map_err(|_| WorldError::InvalidData("world name is not valid UTF-8".into()))?;
    if name.chars().any(char::is_control) {
        return invalid("world name contains control characters");
    }
    Ok(name.to_owned())
}

fn encode_objects(world: &World) -> Vec<u8> {
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

fn decode_objects(world: &mut World, bytes: &[u8], version: u16) -> Result<(), WorldError> {
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
    let object_record_size = if version >= MOTION_VERSION {
        OBJECT_RECORD_SIZE
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
        if !active
            && furniture_definition(object_type)
                .is_some_and(|definition| definition.interaction().is_activatable())
        {
            next_update_tick = u64::MAX;
        }
        if machine_target_y != u32::MAX
            && (object_type != FurnitureObject::LASER_BORE || machine_target_y >= world.height)
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
                if world.tile_in_bounds(x, y, super::Layer::Foreground) != TileId::EMPTY
                    && furniture_definition(object_type).is_none()
                    && !matches!(
                        object_type,
                        super::ROPE_OBJECT | super::POWERED_CABLE_OBJECT
                    )
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
                && TargetPriority::from_raw(variant).is_none()
            {
                return invalid("furniture target priority is invalid");
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
                        if world.tile_in_bounds(x, expected_root_y, super::Layer::Foreground)
                            == TileId::EMPTY
                        {
                            return invalid("furniture floor support tile is empty");
                        }
                    }
                }
                FurnitureSupport::Side => {
                    let valid_root = if root == anchor {
                        world.tile_in_bounds(anchor.x, anchor.y, super::Layer::Background)
                            != TileId::EMPTY
                    } else {
                        let dx = root.x.abs_diff(anchor.x);
                        let dy = root.y.abs_diff(anchor.y);
                        dx + dy == 1
                            && world.tile_in_bounds(root.x, root.y, super::Layer::Foreground)
                                != TileId::EMPTY
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
            if kill_count != 0 && object_type != FurnitureObject::TURRET {
                return invalid("non-turret furniture contains a kill count");
            }
            if object_type == FurnitureObject::CARGO_LIFT {
                if linked_object == 0
                    || super::CargoLiftDirection::from_raw(variant).is_none()
                    || motion_position_milli / 1_000 >= world.height
                    || motion_position_milli.saturating_add(500) / 1_000 != anchor.y
                {
                    return invalid("cargo lift motion state is invalid");
                }
            } else if object_type == FurnitureObject::LIFT_STATION {
                if version < VERSION
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
        } else if object_type != super::POWERED_CABLE_OBJECT
            && world.tile_in_bounds(root.x, root.y, super::Layer::Foreground) == TileId::EMPTY
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
        if object.object_type == super::POWERED_CABLE_OBJECT {
            let expected_root = object
                .anchor
                .y
                .checked_sub(1)
                .map(|y| TilePos::new(object.anchor.x, y));
            if object.width != 1
                || expected_root != Some(object.root)
                || store
                    .occupying(object.root)
                    .and_then(|anchor| store.object(anchor))
                    .is_none_or(|anchor| {
                        anchor.object_type != FurnitureObject::POWERED_CABLE_ANCHOR
                    })
            {
                return invalid("powered cable is not attached to its top anchor");
            }
        }
        if object.object_type == FurnitureObject::CARGO_LIFT {
            let cable = store
                .object(ObjectId::from_raw(object.linked_object))
                .filter(|cable| cable.object_type == super::POWERED_CABLE_OBJECT)
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
                .filter(|cable| cable.object_type == super::POWERED_CABLE_OBJECT)
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

fn decode_containers(
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

fn encode_layer(tiles: &[TileId]) -> Vec<u8> {
    // Terrain normally has long runs; this avoids the first few small reallocations
    // without pessimistically reserving the alternating-tile worst case.
    let mut output = Vec::with_capacity(CHUNK_SIZE * 8);
    let mut index = 0;
    while index < tiles.len() {
        let tile = tiles[index];
        let mut run = 1_usize;
        while index + run < tiles.len() && tiles[index + run] == tile && run < u16::MAX as usize {
            run += 1;
        }
        output.extend_from_slice(&tile.raw().to_le_bytes());
        output.extend_from_slice(&(run as u16).to_le_bytes());
        index += run;
    }
    output
}

fn decode_layer(bytes: &[u8], output: &mut [TileId]) -> Result<(), String> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return Err("malformed RLE tile plane".into());
    }
    if output.len() != CHUNK_AREA {
        return Err("tile plane output has the wrong size".into());
    }
    let mut output_index = 0_usize;
    for pair in bytes.chunks_exact(4) {
        let tile = TileId::new(u16::from_le_bytes([pair[0], pair[1]]));
        let run = u16::from_le_bytes([pair[2], pair[3]]) as usize;
        let end = output_index.saturating_add(run);
        if run == 0 || end > output.len() {
            return Err("invalid RLE run length".into());
        }
        output[output_index..end].fill(tile);
        output_index = end;
    }
    if output_index != output.len() {
        return Err("RLE plane does not contain exactly one chunk".into());
    }
    Ok(())
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811C_9DC5, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, count: usize) -> Result<&'a [u8], WorldError> {
    let end = cursor
        .checked_add(count)
        .ok_or_else(|| WorldError::InvalidData("file offset overflow".into()))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| WorldError::InvalidData("unexpected end of file".into()))?;
    *cursor = end;
    Ok(value)
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, WorldError> {
    let value: [u8; 2] = take(bytes, cursor, 2)?.try_into().unwrap();
    Ok(u16::from_le_bytes(value))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, WorldError> {
    let value: [u8; 4] = take(bytes, cursor, 4)?.try_into().unwrap();
    Ok(u32::from_le_bytes(value))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, WorldError> {
    let value: [u8; 8] = take(bytes, cursor, 8)?.try_into().unwrap();
    Ok(u64::from_le_bytes(value))
}

fn invalid<T>(message: impl Into<String>) -> Result<T, WorldError> {
    Err(WorldError::InvalidData(message.into()))
}

fn sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(
        ".{name}.{}.{}.{}",
        std::process::id(),
        nonce,
        suffix
    ))
}

fn replace_file(temporary: &Path, destination: &Path) -> Result<(), WorldError> {
    if !destination.exists() {
        fs::rename(temporary, destination)?;
        return Ok(());
    }
    let backup = sibling_path(destination, "backup");
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(temporary, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(error.into());
    }
    fs::remove_file(backup)?;
    Ok(())
}

#[cfg(test)]
mod tests;
