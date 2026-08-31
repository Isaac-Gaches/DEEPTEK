mod objects;

use super::{
    BiomeId, CHUNK_AREA, CHUNK_SIZE, FurnitureConfiguration, FurnitureObject, FurnitureSupport,
    LaserDrillAim, Layer, LiftStationConfiguration, MAX_WORLD_NAME_BYTES, ObjectId, ObjectTypeId,
    TargetPriority, TileId, TilePos, World, WorldError, WorldObject, configuration_variant,
    furniture_definition, objects::ObjectStore, parallel_mut,
};
use crate::contracts::SavedContractObjective;
use crate::items::{HOTBAR_SLOTS, INVENTORY_SLOTS, Inventory, ItemContainer, ItemId, ItemStack};
use crate::tutorial::SavedProspectorProgress;
use crate::{
    BUILT_IN_SPECIALISTS, Contract, ContractBoard, ContractCompany, ContractId,
    CorporationProgress, DeliverySystem, SpecialistId, Transmission, TransmissionLog,
    TutorialProgram, specialist_definition,
};
use objects::{decode_objects, encode_objects};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 8] = b"DTKWLD\0\0";
const VERSION: u16 = 19;
const PROSPECTOR_PROGRESS_VERSION: u16 = 19;
const MISSION_STATE_VERSION: u16 = 18;
const BIOMES_VERSION: u16 = 17;
const BLOCK_DURABILITY_VERSION: u16 = 16;
const SPECIALISTS_VERSION: u16 = 15;
const CORPORATION_PROGRESS_VERSION: u16 = 14;
const PLAYER_STATE_VERSION: u16 = 13;
const MACHINE_HEALTH_VERSION: u16 = 12;
const LIFT_STATION_VERSION: u16 = 11;
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
const LEGACY_PLAYER_STATE_SIZE: usize = 16 + INVENTORY_SLOTS * CONTAINER_SLOT_SIZE;
const CORPORATION_PROGRESS_SIZE: usize = ContractCompany::ALL.len() * 4;
const PLAYER_STATE_SIZE: usize = LEGACY_PLAYER_STATE_SIZE + CORPORATION_PROGRESS_SIZE;
const MAX_PLAYER_STATE_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;
const MAX_CONTRACTS_PER_BOARD_LIST: usize = 4_096;
const MAX_CONTRACT_REQUIREMENT_BYTES: usize = 4_096;
const MAX_TRANSMISSIONS: usize = 4_096;
const MAX_TRANSMISSION_TEXT_BYTES: usize = 64 * 1024;
const RECORD_SIZE: usize = 16;
const OBJECT_HEADER_SIZE: usize = 32;
const LEGACY_OBJECT_RECORD_SIZE: usize = 40;
const ACTIVATION_OBJECT_RECORD_SIZE: usize = 41;
const BATTERY_OBJECT_RECORD_SIZE: usize = 49;
const STATISTICS_OBJECT_RECORD_SIZE: usize = 53;
const MOTION_OBJECT_RECORD_SIZE: usize = 65;
const OBJECT_RECORD_SIZE: usize = 67;
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
    let specialist_payload = encode_specialists(world)?;
    let block_damage_payload = encode_block_damage(world)?;
    let biome_payload = encode_biomes(world);

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
        write_player_state(&mut file, world.player_state())?;
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
        file.write_all(&(specialist_payload.len() as u32).to_le_bytes())?;
        file.write_all(&checksum(&specialist_payload).to_le_bytes())?;
        file.write_all(&specialist_payload)?;
        file.write_all(&(block_damage_payload.len() as u32).to_le_bytes())?;
        file.write_all(&checksum(&block_damage_payload).to_le_bytes())?;
        file.write_all(&block_damage_payload)?;
        file.write_all(&(biome_payload.len() as u32).to_le_bytes())?;
        file.write_all(&checksum(&biome_payload).to_le_bytes())?;
        file.write_all(&biome_payload)?;
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
            | LIFT_STATION_VERSION
            | MACHINE_HEALTH_VERSION
            | PLAYER_STATE_VERSION
            | CORPORATION_PROGRESS_VERSION
            | SPECIALISTS_VERSION
            | BLOCK_DURABILITY_VERSION
            | BIOMES_VERSION
            | MISSION_STATE_VERSION
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
    let player_state = if version >= MISSION_STATE_VERSION {
        let size = read_u32(&bytes, &mut cursor)? as usize;
        if size > MAX_PLAYER_STATE_PAYLOAD_SIZE {
            return invalid("player state exceeds its maximum encoded size");
        }
        Some(read_player_state(
            take(&bytes, &mut cursor, size)?,
            true,
            Some(version),
        )?)
    } else if version >= PLAYER_STATE_VERSION {
        let has_corporation_progress = version >= CORPORATION_PROGRESS_VERSION;
        let size = if has_corporation_progress {
            PLAYER_STATE_SIZE
        } else {
            LEGACY_PLAYER_STATE_SIZE
        };
        Some(read_player_state(
            take(&bytes, &mut cursor, size)?,
            has_corporation_progress,
            None,
        )?)
    } else {
        None
    };
    let mut world = World::empty(width, height, seed)?;
    world.name = name;
    if let Some((time_of_day, player_position)) = session {
        world.set_time_of_day(time_of_day)?;
        world.set_player_position(player_position)?;
    }
    if let Some(player_state) = player_state {
        world.set_player_state(player_state);
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
    let specialist_payload = if version >= SPECIALISTS_VERSION {
        let payload_len = read_u32(&bytes, &mut cursor)? as usize;
        let expected_checksum = read_u32(&bytes, &mut cursor)?;
        let payload = take(&bytes, &mut cursor, payload_len)?;
        if checksum(payload) != expected_checksum {
            return invalid("specialist section checksum mismatch");
        }
        Some(payload)
    } else {
        None
    };
    let block_damage_payload = if version >= BLOCK_DURABILITY_VERSION {
        let payload_len = read_u32(&bytes, &mut cursor)? as usize;
        let expected_checksum = read_u32(&bytes, &mut cursor)?;
        let payload = take(&bytes, &mut cursor, payload_len)?;
        if checksum(payload) != expected_checksum {
            return invalid("block durability section checksum mismatch");
        }
        Some(payload)
    } else {
        None
    };
    let biome_payload = if version >= BIOMES_VERSION {
        let payload_len = read_u32(&bytes, &mut cursor)? as usize;
        let expected_checksum = read_u32(&bytes, &mut cursor)?;
        let payload = take(&bytes, &mut cursor, payload_len)?;
        if checksum(payload) != expected_checksum {
            return invalid("biome section checksum mismatch");
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
    if let Some(payload) = specialist_payload {
        decode_specialists(&mut world, payload)?;
    }
    if let Some(payload) = block_damage_payload {
        decode_block_damage(&mut world, payload)?;
    }
    if let Some(payload) = biome_payload {
        decode_biomes(&mut world, payload)?;
    } else {
        world.generate_biomes();
    }
    Ok(world)
}

fn encode_biomes(world: &World) -> Vec<u8> {
    world
        .biome_map()
        .cells()
        .iter()
        .map(|biome| biome.raw())
        .collect()
}

fn decode_biomes(world: &mut World, bytes: &[u8]) -> Result<(), WorldError> {
    if bytes.len() != world.biome_map().cells().len() {
        return invalid("biome map size does not match world dimensions");
    }
    let mut cells = Vec::with_capacity(bytes.len());
    for &raw in bytes {
        let biome = BiomeId::new(raw);
        if !biome.is_known() {
            return invalid("biome map contains an unknown biome ID");
        }
        cells.push(biome);
    }
    if !world.biomes.replace_cells(cells.into_boxed_slice()) {
        return invalid("biome map size does not match world dimensions");
    }
    Ok(())
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
            | LIFT_STATION_VERSION
            | MACHINE_HEALTH_VERSION
            | PLAYER_STATE_VERSION
            | CORPORATION_PROGRESS_VERSION
            | SPECIALISTS_VERSION
            | BLOCK_DURABILITY_VERSION
            | BIOMES_VERSION
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

fn write_player_state(
    file: &mut File,
    state: Option<&super::PlayerState>,
) -> Result<(), WorldError> {
    let mut payload = Vec::new();
    let Some(state) = state else {
        payload.extend_from_slice(&0_u32.to_le_bytes());
        file.write_all(&(payload.len() as u32).to_le_bytes())?;
        file.write_all(&payload)?;
        return Ok(());
    };
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&state.health_current().to_le_bytes());
    payload.extend_from_slice(&state.health_maximum().to_le_bytes());
    payload.extend_from_slice(&(state.inventory().selected_hotbar() as u16).to_le_bytes());
    payload.extend_from_slice(&(INVENTORY_SLOTS as u16).to_le_bytes());
    let (cursor_item, cursor_quantity) = state
        .cursor_stack()
        .map(|stack| (stack.item().raw(), stack.quantity()))
        .unwrap_or((0, 0));
    payload.extend_from_slice(&cursor_item.to_le_bytes());
    payload.extend_from_slice(&cursor_quantity.to_le_bytes());
    for slot in state.inventory().slots() {
        let (item, quantity) = slot
            .map(|stack| (stack.item().raw(), stack.quantity()))
            .unwrap_or((0, 0));
        payload.extend_from_slice(&item.to_le_bytes());
        payload.extend_from_slice(&quantity.to_le_bytes());
    }
    for experience in state.corporation_progress().all_experience() {
        payload.extend_from_slice(&experience.to_le_bytes());
    }
    encode_mission_state(state, &mut payload)?;
    if payload.len() > MAX_PLAYER_STATE_PAYLOAD_SIZE {
        return invalid("player state exceeds its maximum encoded size");
    }
    file.write_all(&(payload.len() as u32).to_le_bytes())?;
    file.write_all(&payload)?;
    Ok(())
}

fn read_player_state(
    bytes: &[u8],
    has_corporation_progress: bool,
    mission_state_version: Option<u16>,
) -> Result<Option<super::PlayerState>, WorldError> {
    let mut cursor = 0;
    let present = read_u32(bytes, &mut cursor)?;
    if present > 1 {
        return invalid("invalid player state flag");
    }
    if present == 0 {
        if mission_state_version.is_none() && bytes[cursor..].iter().any(|&byte| byte != 0) {
            return invalid("absent player state contains data");
        }
        if mission_state_version.is_some() && cursor != bytes.len() {
            return invalid("absent player state contains data");
        }
        return Ok(None);
    }
    let health_current = read_u16(bytes, &mut cursor)?;
    let health_maximum = read_u16(bytes, &mut cursor)?;
    let selected_hotbar = usize::from(read_u16(bytes, &mut cursor)?);
    let slot_count = usize::from(read_u16(bytes, &mut cursor)?);
    if slot_count != INVENTORY_SLOTS || selected_hotbar >= HOTBAR_SLOTS {
        return invalid("player inventory dimensions are invalid");
    }
    let cursor_item = read_u16(bytes, &mut cursor)?;
    let cursor_quantity = read_u16(bytes, &mut cursor)?;
    let cursor_stack = match (cursor_item, cursor_quantity) {
        (0, 0) => None,
        (0, _) | (_, 0) => return invalid("player cursor contains a partial empty stack"),
        (item, quantity) => ItemStack::new(ItemId::new(item), quantity),
    };
    let mut slots = Vec::with_capacity(INVENTORY_SLOTS);
    for _ in 0..INVENTORY_SLOTS {
        let item = read_u16(bytes, &mut cursor)?;
        let quantity = read_u16(bytes, &mut cursor)?;
        slots.push(match (item, quantity) {
            (0, 0) => None,
            (0, _) | (_, 0) => return invalid("player inventory contains a partial empty stack"),
            (item, quantity) => ItemStack::new(ItemId::new(item), quantity),
        });
    }
    let inventory = Inventory::from_saved_slots(slots, selected_hotbar)
        .ok_or_else(|| WorldError::InvalidData("player inventory dimensions are invalid".into()))?;
    let corporation_progress = if has_corporation_progress {
        let mut experience = [0; ContractCompany::ALL.len()];
        for value in &mut experience {
            *value = read_u32(bytes, &mut cursor)?;
        }
        CorporationProgress::from_experience(experience)
    } else {
        CorporationProgress::default()
    };
    let mission_state = mission_state_version
        .map(|version| decode_mission_state(bytes, &mut cursor, version))
        .transpose()?;
    if cursor != bytes.len() {
        return invalid("player state contains trailing data");
    }
    super::PlayerState::new(health_current, health_maximum, inventory)
        .map(|state| {
            let state = state
                .with_cursor_stack(cursor_stack)
                .with_corporation_progress(corporation_progress);
            if let Some((contracts, transmissions, tutorial, deliveries)) = mission_state {
                state.with_mission_state(contracts, transmissions, tutorial, deliveries)
            } else {
                state
            }
        })
        .map(Some)
        .ok_or_else(|| WorldError::InvalidData("player health is invalid".into()))
}

fn encode_mission_state(
    state: &super::PlayerState,
    output: &mut Vec<u8>,
) -> Result<(), WorldError> {
    encode_contracts(state.contract_board().available(), output)?;
    encode_contracts(state.contract_board().active(), output)?;

    let transmissions = state.transmission_log();
    let history_count = u32::try_from(transmissions.history().len())
        .map_err(|_| WorldError::InvalidData("too many transmissions".into()))?;
    if transmissions.history().len() > MAX_TRANSMISSIONS {
        return invalid("too many transmissions");
    }
    output.extend_from_slice(&history_count.to_le_bytes());
    for transmission in transmissions.history() {
        output.extend_from_slice(&transmission.sequence().to_le_bytes());
        encode_bounded_string(transmission.sender(), MAX_TRANSMISSION_TEXT_BYTES, output)?;
        encode_bounded_string(transmission.subject(), MAX_TRANSMISSION_TEXT_BYTES, output)?;
        encode_bounded_string(transmission.body(), MAX_TRANSMISSION_TEXT_BYTES, output)?;
    }
    let (incoming, queued) = transmissions.saved_indices();
    output.extend_from_slice(
        &incoming
            .map_or(u32::MAX, |index| u32::try_from(index).unwrap_or(u32::MAX))
            .to_le_bytes(),
    );
    let queued: Vec<_> = queued.collect();
    let queued_count = u32::try_from(queued.len())
        .map_err(|_| WorldError::InvalidData("too many queued transmissions".into()))?;
    output.extend_from_slice(&queued_count.to_le_bytes());
    for index in queued {
        let index = u32::try_from(index)
            .map_err(|_| WorldError::InvalidData("transmission index is too large".into()))?;
        output.extend_from_slice(&index.to_le_bytes());
    }

    let (tutorial_stage, tutorial_remaining, progress) = state.tutorial_program().saved_state();
    output.push(tutorial_stage);
    output.extend_from_slice(&tutorial_remaining.to_bits().to_le_bytes());
    for value in [
        progress.completed_missions,
        progress.issued_packages,
        progress.facts,
        progress.stone_extracted,
        progress.stone_exported,
        progress.iron_acquired,
        progress.iron_processed,
        progress.asterite_acquired,
        progress.asterite_exported,
    ] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(&progress.maximum_depth_decimetres.to_le_bytes());

    let (delivery_elapsed, drop_sequence, deliveries) = state.delivery_system().saved_state();
    output.extend_from_slice(&delivery_elapsed.to_bits().to_le_bytes());
    output.extend_from_slice(&drop_sequence.to_le_bytes());
    let deliveries: Vec<_> = deliveries.collect();
    let delivery_count = u16::try_from(deliveries.len())
        .map_err(|_| WorldError::InvalidData("too many pending deliveries".into()))?;
    output.extend_from_slice(&delivery_count.to_le_bytes());
    for (item, remaining) in deliveries {
        output.extend_from_slice(&item.raw().to_le_bytes());
        output.extend_from_slice(&remaining.to_bits().to_le_bytes());
    }
    Ok(())
}

fn encode_contracts(contracts: &[Contract], output: &mut Vec<u8>) -> Result<(), WorldError> {
    if contracts.len() > MAX_CONTRACTS_PER_BOARD_LIST {
        return invalid("too many contracts");
    }
    output.extend_from_slice(&(contracts.len() as u32).to_le_bytes());
    for contract in contracts {
        output.push(contract.id().map_or(0, contract_id_tag));
        encode_bounded_string(
            &contract.requirement,
            MAX_CONTRACT_REQUIREMENT_BYTES,
            output,
        )?;
        output.extend_from_slice(&contract.reward.to_le_bytes());
        output.push(match contract.company {
            ContractCompany::DeepTekIndustries => 0,
            ContractCompany::VanguardDefence => 1,
            ContractCompany::AstraSurveyCorp => 2,
        });
        output.extend_from_slice(&contract.experience_reward.to_le_bytes());
        match contract.saved_objective() {
            SavedContractObjective::None => output.push(0),
            SavedContractObjective::ExportItems {
                item,
                exported,
                required,
            } => {
                output.push(1);
                encode_item_progress(item, exported, required, output);
            }
            SavedContractObjective::MineItems {
                item,
                mined,
                required,
            } => {
                output.push(2);
                encode_item_progress(item, mined, required, output);
            }
            SavedContractObjective::BuildAndExport {
                required_objects,
                placed_objects,
                item,
                exported,
                required,
            } => {
                output.push(3);
                encode_object_types(&required_objects, output)?;
                encode_object_types(&placed_objects, output)?;
                encode_item_progress(item, exported, required, output);
            }
            SavedContractObjective::Program {
                completed,
                required,
            } => {
                output.push(4);
                output.extend_from_slice(&completed.to_le_bytes());
                output.extend_from_slice(&required.to_le_bytes());
            }
        }
    }
    Ok(())
}

fn encode_item_progress(item: ItemId, progress: u64, required: u64, output: &mut Vec<u8>) {
    output.extend_from_slice(&item.raw().to_le_bytes());
    output.extend_from_slice(&progress.to_le_bytes());
    output.extend_from_slice(&required.to_le_bytes());
}

fn encode_object_types(
    object_types: &[ObjectTypeId],
    output: &mut Vec<u8>,
) -> Result<(), WorldError> {
    let count = u16::try_from(object_types.len())
        .map_err(|_| WorldError::InvalidData("too many contract object requirements".into()))?;
    output.extend_from_slice(&count.to_le_bytes());
    for object_type in object_types {
        output.extend_from_slice(&object_type.raw().to_le_bytes());
    }
    Ok(())
}

fn encode_bounded_string(
    value: &str,
    maximum_bytes: usize,
    output: &mut Vec<u8>,
) -> Result<(), WorldError> {
    if value.len() > maximum_bytes {
        return invalid("mission text exceeds its maximum encoded length");
    }
    output.extend_from_slice(&(value.len() as u32).to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn decode_mission_state(
    bytes: &[u8],
    cursor: &mut usize,
    version: u16,
) -> Result<
    (
        ContractBoard,
        TransmissionLog,
        TutorialProgram,
        DeliverySystem,
    ),
    WorldError,
> {
    let available = decode_contracts(bytes, cursor)?;
    let active = decode_contracts(bytes, cursor)?;
    let contracts = ContractBoard::from_saved(available, active)
        .ok_or_else(|| WorldError::InvalidData("contract board is invalid".into()))?;

    let history_count = read_u32(bytes, cursor)? as usize;
    if history_count > MAX_TRANSMISSIONS {
        return invalid("too many transmissions");
    }
    let mut history = Vec::with_capacity(history_count);
    for _ in 0..history_count {
        history.push(Transmission::from_saved(
            read_u64(bytes, cursor)?,
            decode_bounded_string(bytes, cursor, MAX_TRANSMISSION_TEXT_BYTES)?,
            decode_bounded_string(bytes, cursor, MAX_TRANSMISSION_TEXT_BYTES)?,
            decode_bounded_string(bytes, cursor, MAX_TRANSMISSION_TEXT_BYTES)?,
        ));
    }
    let incoming = match read_u32(bytes, cursor)? {
        u32::MAX => None,
        index => Some(index as usize),
    };
    let queued_count = read_u32(bytes, cursor)? as usize;
    if queued_count > MAX_TRANSMISSIONS {
        return invalid("too many queued transmissions");
    }
    let mut queued = VecDeque::with_capacity(queued_count);
    for _ in 0..queued_count {
        queued.push_back(read_u32(bytes, cursor)? as usize);
    }
    let transmissions = TransmissionLog::from_saved(history, incoming, queued)
        .ok_or_else(|| WorldError::InvalidData("transmission log is invalid".into()))?;

    let tutorial_stage = *take(bytes, cursor, 1)?
        .first()
        .expect("one tutorial stage byte was taken");
    let tutorial_remaining = f32::from_bits(read_u32(bytes, cursor)?);
    let progress = if version >= PROSPECTOR_PROGRESS_VERSION {
        Some(SavedProspectorProgress {
            completed_missions: read_u64(bytes, cursor)?,
            issued_packages: read_u64(bytes, cursor)?,
            facts: read_u64(bytes, cursor)?,
            stone_extracted: read_u64(bytes, cursor)?,
            stone_exported: read_u64(bytes, cursor)?,
            iron_acquired: read_u64(bytes, cursor)?,
            iron_processed: read_u64(bytes, cursor)?,
            asterite_acquired: read_u64(bytes, cursor)?,
            asterite_exported: read_u64(bytes, cursor)?,
            maximum_depth_decimetres: read_u32(bytes, cursor)?,
        })
    } else {
        None
    };
    let tutorial = TutorialProgram::from_saved(tutorial_stage, tutorial_remaining, progress)
        .ok_or_else(|| WorldError::InvalidData("tutorial state is invalid".into()))?;

    let delivery_elapsed = f32::from_bits(read_u32(bytes, cursor)?);
    let drop_sequence = read_u32(bytes, cursor)?;
    let delivery_count = read_u16(bytes, cursor)? as usize;
    let mut pending = Vec::with_capacity(delivery_count);
    for _ in 0..delivery_count {
        pending.push((
            ItemId::new(read_u16(bytes, cursor)?),
            f32::from_bits(read_u32(bytes, cursor)?),
        ));
    }
    let deliveries = DeliverySystem::from_saved(delivery_elapsed, drop_sequence, pending)
        .ok_or_else(|| WorldError::InvalidData("delivery queue is invalid".into()))?;
    Ok((contracts, transmissions, tutorial, deliveries))
}

fn decode_contracts(bytes: &[u8], cursor: &mut usize) -> Result<Vec<Contract>, WorldError> {
    let count = read_u32(bytes, cursor)? as usize;
    if count > MAX_CONTRACTS_PER_BOARD_LIST {
        return invalid("too many contracts");
    }
    let mut contracts = Vec::with_capacity(count);
    for _ in 0..count {
        let id = match *take(bytes, cursor, 1)?
            .first()
            .expect("one contract ID byte was taken")
        {
            0 => None,
            tag => Some(contract_id_from_tag(tag).ok_or_else(|| {
                WorldError::InvalidData("contract contains an unknown ID".into())
            })?),
        };
        let requirement = decode_bounded_string(bytes, cursor, MAX_CONTRACT_REQUIREMENT_BYTES)?;
        let reward = read_u64(bytes, cursor)?;
        let company = match *take(bytes, cursor, 1)?
            .first()
            .expect("one contract company byte was taken")
        {
            0 => ContractCompany::DeepTekIndustries,
            1 => ContractCompany::VanguardDefence,
            2 => ContractCompany::AstraSurveyCorp,
            _ => return invalid("contract contains an unknown company"),
        };
        let experience_reward = read_u32(bytes, cursor)?;
        let objective_tag = *take(bytes, cursor, 1)?
            .first()
            .expect("one contract objective byte was taken");
        let objective = match objective_tag {
            0 => SavedContractObjective::None,
            1 => {
                let (item, progress, required) = decode_item_progress(bytes, cursor)?;
                SavedContractObjective::ExportItems {
                    item,
                    exported: progress,
                    required,
                }
            }
            2 => {
                let (item, progress, required) = decode_item_progress(bytes, cursor)?;
                SavedContractObjective::MineItems {
                    item,
                    mined: progress,
                    required,
                }
            }
            3 => {
                let required_objects = decode_object_types(bytes, cursor)?;
                let placed_objects = decode_object_types(bytes, cursor)?;
                let (item, exported, required) = decode_item_progress(bytes, cursor)?;
                SavedContractObjective::BuildAndExport {
                    required_objects,
                    placed_objects,
                    item,
                    exported,
                    required,
                }
            }
            4 => SavedContractObjective::Program {
                completed: read_u16(bytes, cursor)?,
                required: read_u16(bytes, cursor)?,
            },
            _ => return invalid("contract contains an unknown objective"),
        };
        contracts.push(
            Contract::from_saved(
                id,
                requirement,
                reward,
                company,
                experience_reward,
                objective,
            )
            .ok_or_else(|| WorldError::InvalidData("contract state is invalid".into()))?,
        );
    }
    Ok(contracts)
}

const fn contract_id_tag(id: ContractId) -> u8 {
    match id {
        ContractId::BreakingGround => 1,
        ContractId::FirstShipment => 2,
        ContractId::SitePower => 3,
        ContractId::Procurement => 4,
        ContractId::IndustrialExtraction => 5,
        ContractId::Prospecting => 6,
        ContractId::IronAge => 7,
        ContractId::GoingDown => 8,
        ContractId::ValueAdded => 9,
        ContractId::HandsOff => 10,
        ContractId::HelpWanted => 11,
        ContractId::Depth100 => 12,
        ContractId::Depth250 => 13,
        ContractId::Depth500 => 14,
        ContractId::Depth1000 => 15,
        ContractId::Depth2500 => 16,
        ContractId::Depth5000 => 17,
        ContractId::RecoverAsterite => 18,
    }
}

const fn contract_id_from_tag(tag: u8) -> Option<ContractId> {
    Some(match tag {
        1 => ContractId::BreakingGround,
        2 => ContractId::FirstShipment,
        3 => ContractId::SitePower,
        4 => ContractId::Procurement,
        5 => ContractId::IndustrialExtraction,
        6 => ContractId::Prospecting,
        7 => ContractId::IronAge,
        8 => ContractId::GoingDown,
        9 => ContractId::ValueAdded,
        10 => ContractId::HandsOff,
        11 => ContractId::HelpWanted,
        12 => ContractId::Depth100,
        13 => ContractId::Depth250,
        14 => ContractId::Depth500,
        15 => ContractId::Depth1000,
        16 => ContractId::Depth2500,
        17 => ContractId::Depth5000,
        18 => ContractId::RecoverAsterite,
        _ => return None,
    })
}

fn decode_item_progress(
    bytes: &[u8],
    cursor: &mut usize,
) -> Result<(ItemId, u64, u64), WorldError> {
    let item = ItemId::new(read_u16(bytes, cursor)?);
    let progress = read_u64(bytes, cursor)?;
    let required = read_u64(bytes, cursor)?;
    Ok((item, progress, required))
}

fn decode_object_types(bytes: &[u8], cursor: &mut usize) -> Result<Vec<ObjectTypeId>, WorldError> {
    let count = read_u16(bytes, cursor)? as usize;
    let mut object_types = Vec::with_capacity(count);
    for _ in 0..count {
        object_types.push(ObjectTypeId::new(read_u16(bytes, cursor)?));
    }
    Ok(object_types)
}

fn decode_bounded_string(
    bytes: &[u8],
    cursor: &mut usize,
    maximum_bytes: usize,
) -> Result<String, WorldError> {
    let length = read_u32(bytes, cursor)? as usize;
    if length > maximum_bytes {
        return invalid("mission text exceeds its maximum encoded length");
    }
    let value = std::str::from_utf8(take(bytes, cursor, length)?)
        .map_err(|_| WorldError::InvalidData("mission text is not valid UTF-8".into()))?;
    Ok(value.to_owned())
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

fn encode_specialists(world: &World) -> Result<Vec<u8>, WorldError> {
    let count = u16::try_from(world.specialists().len())
        .map_err(|_| WorldError::InvalidData("too many specialists".into()))?;
    let mut seen = std::collections::HashSet::new();
    let mut output = Vec::with_capacity(2 + usize::from(count) * 19);
    output.extend_from_slice(&count.to_le_bytes());
    for specialist in world.specialists() {
        if specialist_definition(specialist.id()).is_none() || !seen.insert(specialist.id()) {
            return invalid("specialist records contain an unknown or duplicate ID");
        }
        if !world
            .object(specialist.home_terminal())
            .is_some_and(|object| object.object_type() == FurnitureObject::PROCUREMENT_TERMINAL)
        {
            return invalid("specialist home references a missing terminal");
        }
        let [x, y] = specialist.position();
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..world.width() as f32).contains(&x)
            || !(0.0..world.height() as f32).contains(&y)
            || specialist.happiness() > 100
        {
            return invalid("specialist state is invalid");
        }
        output.extend_from_slice(&specialist.id().raw().to_le_bytes());
        output.extend_from_slice(&specialist.home_terminal().raw().to_le_bytes());
        output.extend_from_slice(&x.to_bits().to_le_bytes());
        output.extend_from_slice(&y.to_bits().to_le_bytes());
        output.push(specialist.happiness());
    }
    Ok(output)
}

fn decode_specialists(world: &mut World, bytes: &[u8]) -> Result<(), WorldError> {
    let mut cursor = 0;
    let count = usize::from(read_u16(bytes, &mut cursor)?);
    if count > BUILT_IN_SPECIALISTS.len() || bytes.len() != 2 + count * 19 {
        return invalid("specialist section has invalid dimensions");
    }
    let mut seen = std::collections::HashSet::new();
    let mut specialists = Vec::with_capacity(count);
    for _ in 0..count {
        let id = SpecialistId::new(read_u16(bytes, &mut cursor)?);
        let home = ObjectId::from_raw(read_u64(bytes, &mut cursor)?);
        let position = [
            f32::from_bits(read_u32(bytes, &mut cursor)?),
            f32::from_bits(read_u32(bytes, &mut cursor)?),
        ];
        let happiness = take(bytes, &mut cursor, 1)?[0];
        if specialist_definition(id).is_none() || !seen.insert(id) {
            return invalid("specialist section contains an unknown or duplicate ID");
        }
        if !world
            .object(home)
            .is_some_and(|object| object.object_type() == FurnitureObject::PROCUREMENT_TERMINAL)
        {
            return invalid("specialist home references a missing terminal");
        }
        if !position.into_iter().all(f32::is_finite)
            || !(0.0..world.width() as f32).contains(&position[0])
            || !(0.0..world.height() as f32).contains(&position[1])
            || happiness > 100
        {
            return invalid("specialist section contains invalid state");
        }
        specialists.push(crate::SpecialistRecord::from_saved(
            id, home, position, happiness,
        ));
    }
    world.specialists = specialists;
    Ok(())
}

const BLOCK_DAMAGE_RECORD_SIZE: usize = 11;

fn encode_block_damage(world: &World) -> Result<Vec<u8>, WorldError> {
    let mut entries: Vec<_> = world.block_damage_entries().collect();
    entries.sort_unstable_by_key(|(key, _)| {
        let layer = match key.layer {
            Layer::Foreground => 0,
            Layer::Background => 1,
        };
        (key.position.y, key.position.x, layer)
    });
    let count = u32::try_from(entries.len())
        .map_err(|_| WorldError::InvalidData("too many damaged blocks".into()))?;
    let mut output = Vec::with_capacity(4 + entries.len() * BLOCK_DAMAGE_RECORD_SIZE);
    output.extend_from_slice(&count.to_le_bytes());
    for (key, damage) in entries {
        let valid = world
            .block_health(key.position, key.layer)?
            .is_some_and(|health| health.damage() == damage && damage < health.maximum());
        if !valid {
            return invalid("block damage store contains invalid state");
        }
        output.extend_from_slice(&key.position.x.to_le_bytes());
        output.extend_from_slice(&key.position.y.to_le_bytes());
        output.push(match key.layer {
            Layer::Foreground => 0,
            Layer::Background => 1,
        });
        output.extend_from_slice(&damage.to_le_bytes());
    }
    Ok(output)
}

fn decode_block_damage(world: &mut World, bytes: &[u8]) -> Result<(), WorldError> {
    let mut cursor = 0;
    let count = read_u32(bytes, &mut cursor)? as usize;
    let expected_len = 4_usize
        .checked_add(
            count
                .checked_mul(BLOCK_DAMAGE_RECORD_SIZE)
                .ok_or_else(|| WorldError::InvalidData("block damage size overflow".into()))?,
        )
        .ok_or_else(|| WorldError::InvalidData("block damage size overflow".into()))?;
    let maximum_records = u64::from(world.width()) * u64::from(world.height()) * 2;
    if bytes.len() != expected_len || count as u64 > maximum_records {
        return invalid("block durability section has invalid dimensions");
    }
    for _ in 0..count {
        let position = TilePos::new(read_u32(bytes, &mut cursor)?, read_u32(bytes, &mut cursor)?);
        let layer = match take(bytes, &mut cursor, 1)?[0] {
            0 => Layer::Foreground,
            1 => Layer::Background,
            _ => return invalid("block durability section has an invalid layer"),
        };
        let damage = read_u16(bytes, &mut cursor)?;
        world.restore_block_damage(position, layer, damage)?;
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
