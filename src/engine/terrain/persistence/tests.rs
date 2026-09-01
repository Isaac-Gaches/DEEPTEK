use super::*;
use crate::PowerSystem;
use crate::terrain::{
    BackgroundTile, ForegroundTile, FurnitureObject, Layer, TargetPriority, TilePos, WorldGenerator,
};
use std::time::Duration;

fn temp_world_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "deeptek-{label}-{}-{}.world",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn remove_activation_flags(bytes: &mut Vec<u8>, payload_start: usize, object_count: usize) {
    const ACTIVE_FLAG_OFFSET: usize = 12;
    for index in (0..object_count).rev() {
        bytes.remove(
            payload_start + OBJECT_HEADER_SIZE + index * OBJECT_RECORD_SIZE + ACTIVE_FLAG_OFFSET,
        );
    }
}

fn remove_player_state(bytes: &mut Vec<u8>, world_name_length: usize) {
    let start = HEADER_SIZE + world_name_length + SESSION_METADATA_SIZE;
    let payload_size = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()) as usize;
    bytes.drain(start..start + 4 + payload_size);
}

fn remove_empty_surveyed_area_section(bytes: &mut Vec<u8>) {
    const EMPTY_SURVEYED_AREA_SECTION_SIZE: usize = 12;
    let survey_start = bytes.len() - EMPTY_SURVEYED_AREA_SECTION_SIZE;
    assert_eq!(
        u32::from_le_bytes(bytes[survey_start..survey_start + 4].try_into().unwrap()),
        4
    );
    assert_eq!(&bytes[survey_start + 8..], &[0, 0, 0, 0]);
    bytes.truncate(survey_start);
}

fn remove_empty_specialist_section(bytes: &mut Vec<u8>) {
    remove_empty_surveyed_area_section(bytes);
    let chunk_count = u32::from_le_bytes(bytes[28..32].try_into().unwrap()) as usize;
    let biome_section_size = 8 + chunk_count;
    let biome_start = bytes.len() - biome_section_size;
    assert_eq!(
        u32::from_le_bytes(bytes[biome_start..biome_start + 4].try_into().unwrap()) as usize,
        chunk_count
    );
    bytes.truncate(biome_start);

    const EMPTY_BLOCK_DAMAGE_SECTION_SIZE: usize = 12;
    const EMPTY_SECTION_SIZE: usize = 10;
    let block_start = bytes.len() - EMPTY_BLOCK_DAMAGE_SECTION_SIZE;
    assert_eq!(
        u32::from_le_bytes(bytes[block_start..block_start + 4].try_into().unwrap()),
        4
    );
    assert_eq!(&bytes[block_start + 8..], &[0, 0, 0, 0]);
    bytes.truncate(block_start);
    let start = bytes.len() - EMPTY_SECTION_SIZE;
    assert_eq!(
        u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()),
        2
    );
    assert_eq!(&bytes[start + 8..], &[0, 0]);
    bytes.truncate(start);
}

#[test]
fn world_round_trips_across_edge_chunks() {
    let path = temp_world_path("roundtrip");
    let world = WorldGenerator::new(123)
        .with_threads(3)
        .generate(130, 70)
        .unwrap();
    world.save_with_threads(&path, 3).unwrap();
    let loaded = World::load_with_threads(&path, 2).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(world, loaded);
}

#[test]
fn biome_map_round_trips_with_manual_edits() {
    let path = temp_world_path("biomes");
    let mut world = World::empty(130, 130, 91).unwrap();
    assert!(world.set_biome_in_chunk(
        crate::ChunkPos { x: 1, y: 1 },
        crate::BiomeId::GLOWING_CRYSTAL,
    ));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(world.biome_map(), loaded.biome_map());
    assert_eq!(
        loaded.biome_in_chunk(crate::ChunkPos { x: 1, y: 1 }),
        Some(crate::BiomeId::GLOWING_CRYSTAL)
    );
}

#[test]
fn surveyed_map_areas_round_trip() {
    let path = temp_world_path("surveyed-areas");
    let mut world = World::empty(80, 80, 91).unwrap();
    for x in 0..80 {
        world
            .set_tile(x, 20, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let surveyor = world
        .place_furniture(FurnitureObject::SUBSURFACE_SURVEYOR, TilePos::new(38, 18))
        .unwrap();
    world.subsurface_survey(surveyor).unwrap();
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert!(loaded.is_surveyed(TilePos::new(38, 30)));
    assert!(!loaded.is_surveyed(TilePos::new(0, 0)));
    assert_eq!(
        loaded.surveyed_areas().collect::<Vec<_>>(),
        world.surveyed_areas().collect::<Vec<_>>()
    );
}

#[test]
fn version_twenty_worlds_load_without_surveyed_areas() {
    let path = temp_world_path("version-twenty");
    World::empty(8, 8, 92).unwrap().save(&path).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    remove_empty_surveyed_area_section(&mut bytes);
    bytes[8..10].copy_from_slice(&DELAYED_MISSION_TRANSMISSIONS_VERSION.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.seed(), 92);
    assert_eq!(loaded.surveyed_areas().len(), 0);
}

#[test]
fn save_replaces_an_existing_world() {
    let path = temp_world_path("replace");
    World::empty(1, 1, 1).unwrap().save(&path).unwrap();
    World::empty(2, 2, 2).unwrap().save(&path).unwrap();
    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!((loaded.width(), loaded.height(), loaded.seed()), (2, 2, 2));
}

#[test]
fn corruption_is_reported() {
    let path = temp_world_path("corrupt");
    WorldGenerator::new(1)
        .generate(64, 64)
        .unwrap()
        .save(&path)
        .unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(&path, bytes).unwrap();
    let error = World::load(&path).unwrap_err();
    fs::remove_file(path).unwrap();
    assert!(matches!(error, WorldError::InvalidData(_)));
}

#[test]
fn custom_tiles_survive_rle() {
    let path = temp_world_path("custom");
    let mut world = World::empty(3, 3, 4).unwrap();
    world
        .set_tile(2, 2, Layer::Foreground, TileId::new(65_000))
        .unwrap();
    world.save(&path).unwrap();
    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(
        loaded.tile(2, 2, Layer::Foreground).unwrap(),
        TileId::new(65_000)
    );
}

#[test]
fn partial_block_damage_round_trips_without_dense_health_storage() {
    let path = temp_world_path("block-durability");
    let mut world = World::empty(64, 64, 4).unwrap();
    let foreground = TilePos::new(7, 9);
    let background = TilePos::new(8, 9);
    world
        .set_tile(
            foreground.x,
            foreground.y,
            Layer::Foreground,
            ForegroundTile::STONE,
        )
        .unwrap();
    world
        .set_tile(
            background.x,
            background.y,
            Layer::Background,
            BackgroundTile::STONE_WALL,
        )
        .unwrap();
    world
        .damage_block(foreground, Layer::Foreground, 13)
        .unwrap();
    world
        .damage_block(background, Layer::Background, 4)
        .unwrap();
    assert_eq!(world.damaged_block_count(), 2);
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();

    assert_eq!(world, loaded);
    assert_eq!(loaded.damaged_block_count(), 2);
    assert_eq!(
        loaded
            .block_health(foreground, Layer::Foreground)
            .unwrap()
            .unwrap()
            .current(),
        27
    );
    assert_eq!(
        loaded
            .block_health(background, Layer::Background)
            .unwrap()
            .unwrap()
            .current(),
        16
    );
}

#[test]
fn world_name_round_trips_and_can_be_read_without_loading_terrain() {
    let path = temp_world_path("named");
    let mut world = World::empty(3, 3, 4).unwrap();
    world.set_name("Copper Hills").unwrap();
    world.save(&path).unwrap();

    assert_eq!(
        World::read_name(&path).unwrap().as_deref(),
        Some("Copper Hills")
    );
    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.name(), "Copper Hills");
}

#[test]
fn session_time_and_player_position_round_trip() {
    let path = temp_world_path("session");
    let mut world = World::empty(64, 64, 4).unwrap();
    world.set_time_of_day(0.3125).unwrap();
    world.set_player_position(Some([17.25, 28.5])).unwrap();
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.time_of_day(), 0.3125);
    assert_eq!(loaded.player_position(), Some([17.25, 28.5]));
}

#[test]
fn player_health_inventory_and_selected_slot_round_trip_per_world() {
    let path = temp_world_path("player-state");
    let registry = crate::ItemRegistry::with_built_ins();
    let mut inventory = crate::Inventory::new();
    assert_eq!(inventory.add(crate::ItemId::STONE_BLOCK, 37, &registry), 0);
    assert_eq!(inventory.add(crate::ItemId::GLOW_STICK, 9, &registry), 0);
    inventory.select_hotbar(1);
    let mut corporation_progress = CorporationProgress::default();
    corporation_progress.award(ContractCompany::DeepTekIndustries, 275);
    corporation_progress.award(ContractCompany::AstraSurveyCorp, 700);
    let state = crate::PlayerState::new(43, 100, inventory.clone())
        .unwrap()
        .with_cursor_stack(ItemStack::new(crate::ItemId::BOMB, 4))
        .with_corporation_progress(corporation_progress);
    let mut world = World::empty(64, 64, 4).unwrap();
    world.set_player_state(Some(state.clone()));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.player_state(), Some(&state));
    assert_eq!(loaded.player_state().unwrap().inventory(), &inventory);
}

#[test]
fn contracts_transmissions_tutorial_and_deliveries_round_trip_per_world() {
    let path = temp_world_path("mission-state");
    let mut contracts = ContractBoard::with_built_ins();
    contracts.accept(0).unwrap();
    contracts.apply_export(crate::ItemId::STONE_BLOCK, 17);

    let mut transmissions = TransmissionLog::default();
    transmissions.receive(Transmission::new("DEEPTEK", "FIRST", "ONE"));
    transmissions.receive(Transmission::new("DEEPTEK", "SECOND", "TWO"));

    let mut tutorial = TutorialProgram::for_new_world();
    let mut tutorial_contracts = ContractBoard::with_built_ins();
    let mut tutorial_transmissions = TransmissionLog::default();
    let mut tutorial_deliveries = DeliverySystem::default();
    let tutorial_world = World::empty(8, 8, 9).unwrap();
    let tutorial_power = crate::PowerSystem::new();
    let tutorial_inventory = crate::Inventory::new();
    tutorial.update(
        5.0,
        &mut tutorial_contracts,
        &mut tutorial_transmissions,
        &mut tutorial_deliveries,
        &tutorial_world,
        &tutorial_power,
        &tutorial_inventory,
        0,
    );
    tutorial.record_extraction(crate::ItemId::STONE_BLOCK, 20);
    tutorial.update(
        0.0,
        &mut tutorial_contracts,
        &mut tutorial_transmissions,
        &mut tutorial_deliveries,
        &tutorial_world,
        &tutorial_power,
        &tutorial_inventory,
        0,
    );

    let mut deliveries = DeliverySystem::default();
    deliveries
        .schedule_batch(&[crate::ItemId::LASER_BORE], 7.5)
        .unwrap();
    let state = crate::PlayerState::new(100, 100, crate::Inventory::new())
        .unwrap()
        .with_mission_state(contracts, transmissions, tutorial, deliveries);
    let mut world = World::empty(8, 8, 4).unwrap();
    world.set_player_state(Some(state.clone()));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.player_state(), Some(&state));
    assert_eq!(
        loaded.player_state().unwrap().contract_board().active()[0]
            .export_progress()
            .unwrap()
            .exported,
        17
    );
    assert_eq!(
        loaded
            .player_state()
            .unwrap()
            .transmission_log()
            .incoming()
            .unwrap()
            .subject(),
        "FIRST"
    );
}

#[test]
fn version_thirteen_player_state_loads_with_default_corporation_progress() {
    let path = temp_world_path("version-thirteen");
    let mut corporation_progress = CorporationProgress::default();
    corporation_progress.award(ContractCompany::VanguardDefence, 500);
    let state = crate::PlayerState::new(25, 100, crate::Inventory::new())
        .unwrap()
        .with_corporation_progress(corporation_progress);
    let mut world = World::empty(8, 8, 4).unwrap();
    world.set_name("Legacy Corporation Save").unwrap();
    world.set_player_state(Some(state));
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    let state_start = HEADER_SIZE + world.name().len() + SESSION_METADATA_SIZE;
    let payload_size =
        u32::from_le_bytes(bytes[state_start..state_start + 4].try_into().unwrap()) as usize;
    let legacy_state = bytes[state_start + 4..state_start + 4 + LEGACY_PLAYER_STATE_SIZE].to_vec();
    bytes.splice(state_start..state_start + 4 + payload_size, legacy_state);
    bytes[8..10].copy_from_slice(&PLAYER_STATE_VERSION.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    assert_eq!(
        World::read_name(&path).unwrap().as_deref(),
        Some("Legacy Corporation Save")
    );
    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    let loaded_state = loaded.player_state().unwrap();
    assert_eq!(loaded_state.health_current(), 25);
    assert_eq!(
        loaded_state.corporation_progress(),
        CorporationProgress::default()
    );
}

#[test]
fn version_twelve_worlds_load_without_player_state() {
    let path = temp_world_path("version-twelve");
    let mut world = World::empty(8, 8, 4).unwrap();
    world.set_player_state(Some(
        crate::PlayerState::new(25, 100, crate::Inventory::new()).unwrap(),
    ));
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, world.name().len());
    bytes[8..10].copy_from_slice(&MACHINE_HEALTH_VERSION.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.player_state(), None);
}

#[test]
fn version_four_worlds_load_with_default_session_metadata() {
    let path = temp_world_path("version-four");
    let mut world = World::empty(8, 8, 4).unwrap();
    world.set_name("Legacy Name").unwrap();
    world.set_time_of_day(0.2).unwrap();
    world.set_player_position(Some([2.0, 3.0])).unwrap();
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    bytes[8..10].copy_from_slice(&NAMED_WORLD_VERSION.to_le_bytes());
    let session_start = HEADER_SIZE + world.name().len();
    remove_player_state(&mut bytes, world.name().len());
    bytes.drain(session_start..session_start + SESSION_METADATA_SIZE);
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.name(), "Legacy Name");
    assert_eq!(loaded.time_of_day(), super::super::DEFAULT_TIME_OF_DAY);
    assert_eq!(loaded.player_position(), None);
}

#[test]
fn version_one_worlds_remain_loadable_without_objects() {
    let path = temp_world_path("version-one");
    World::empty(1, 1, 8).unwrap().save(&path).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    bytes[8..10].copy_from_slice(&LEGACY_VERSION.to_le_bytes());
    remove_player_state(&mut bytes, 0);
    bytes.drain(HEADER_SIZE..HEADER_SIZE + SESSION_METADATA_SIZE);
    // One empty chunk has two four-byte RLE planes after its record.
    bytes.truncate(HEADER_SIZE + RECORD_SIZE + 8);
    fs::write(&path, bytes).unwrap();
    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.object_count(), 0);
    assert_eq!(loaded.seed(), 8);
}

#[test]
fn version_two_furniture_migrates_with_empty_container_storage() {
    let path = temp_world_path("version-two-furniture");
    let mut world = World::empty(8, 8, 9).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 5, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, 0);
    bytes.drain(HEADER_SIZE..HEADER_SIZE + SESSION_METADATA_SIZE);
    bytes[8..10].copy_from_slice(&OBJECTS_VERSION.to_le_bytes());
    let mut cursor = HEADER_SIZE;
    for _ in 0..world.chunk_count() {
        let foreground_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        let background_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        cursor += 8 + foreground_len + background_len;
    }
    let payload_length_offset = cursor;
    let payload_length = read_u32(&bytes, &mut cursor).unwrap() as usize;
    let _payload_checksum = read_u32(&bytes, &mut cursor).unwrap();
    let payload_start = cursor;
    assert_eq!(payload_start + payload_length, bytes.len());
    let object_count = u32::from_le_bytes(
        bytes[payload_start + 24..payload_start + 28]
            .try_into()
            .unwrap(),
    ) as usize;
    remove_activation_flags(&mut bytes, payload_start, object_count);
    let version_two_payload_len = OBJECT_HEADER_SIZE + object_count * LEGACY_OBJECT_RECORD_SIZE;
    bytes[payload_start + 28..payload_start + 32].copy_from_slice(&0_u32.to_le_bytes());
    bytes[payload_length_offset..payload_length_offset + 4]
        .copy_from_slice(&(version_two_payload_len as u32).to_le_bytes());
    let payload_checksum = checksum(&bytes[payload_start..payload_start + version_two_payload_len]);
    bytes[payload_length_offset + 4..payload_length_offset + 8]
        .copy_from_slice(&payload_checksum.to_le_bytes());
    bytes.truncate(payload_start + version_two_payload_len);
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.object(chest).unwrap().size(), [2, 2]);
    assert!(loaded.container(chest).unwrap().is_empty());
    assert_eq!(loaded.container(chest).unwrap().slots().len(), 40);
}

#[test]
fn furniture_round_trips_with_its_footprint_and_support_index() {
    let path = temp_world_path("furniture");
    let mut world = World::empty(8, 8, 9).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 5, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
    }
    let id = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();
    assert!(
        world
            .container_mut(id)
            .unwrap()
            .set_slot(7, ItemStack::new(ItemId::DIRT_BLOCK, 321),)
    );
    world.save(&path).unwrap();

    let mut loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.object_at(TilePos::new(3, 4)).unwrap().id(), id);
    assert_eq!(
        loaded.container(id).unwrap().slot(7),
        ItemStack::new(ItemId::DIRT_BLOCK, 321)
    );
    assert!(matches!(
        loaded.set_tile(3, 5, Layer::Foreground, TileId::EMPTY),
        Err(WorldError::ContainerNotEmpty { object }) if object == id
    ));
}

#[test]
fn spikes_round_trip_as_floor_supported_furniture() {
    let path = temp_world_path("spikes");
    let mut world = World::empty(8, 8, 11).unwrap();
    world
        .set_tile(3, 6, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let spikes = world
        .place_furniture(FurnitureObject::SPIKES, TilePos::new(3, 5))
        .unwrap();
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    let object = loaded.object(spikes).unwrap();
    assert_eq!(object.object_type(), FurnitureObject::SPIKES);
    assert_eq!(object.anchor(), TilePos::new(3, 5));
    assert_eq!(object.root(), TilePos::new(3, 6));
}

#[test]
fn specialists_round_trip_with_unique_identity_home_and_happiness() {
    let path = temp_world_path("specialists");
    let mut world = World::empty(12, 12, 5).unwrap();
    for x in 4..=5 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let terminal = world
        .place_furniture(FurnitureObject::PROCUREMENT_TERMINAL, TilePos::new(4, 6))
        .unwrap();
    world.specialists.push(crate::SpecialistRecord::from_saved(
        crate::SpecialistId::ENGINEER,
        terminal,
        [4.5, 5.0],
        73,
    ));

    world.save(&path).unwrap();
    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();

    let specialist = loaded.specialist(crate::SpecialistId::ENGINEER).unwrap();
    assert_eq!(specialist.home_terminal(), terminal);
    assert_eq!(specialist.position(), [4.5, 5.0]);
    assert_eq!(specialist.happiness(), 73);
}

#[test]
fn power_furniture_round_trips_and_rebuilds_its_derived_grid() {
    let path = temp_world_path("power-furniture");
    let mut world = World::empty(24, 16, 9).unwrap();
    for x in [2, 3, 9, 11, 12, 15, 16, 17, 18] {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 7))
        .unwrap();
    let pylon = world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(9, 8))
        .unwrap();
    let battery = world
        .place_furniture(FurnitureObject::BATTERY, TilePos::new(11, 8))
        .unwrap();
    assert!(world.set_battery_charge_milli(battery, 123_456));
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(15, 7))
        .unwrap();
    world.save(&path).unwrap();

    let mut loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.object(solar).unwrap().size(), [2, 3]);
    assert_eq!(loaded.object(pylon).unwrap().size(), [1, 2]);
    assert_eq!(loaded.object(battery).unwrap().size(), [2, 2]);
    assert_eq!(loaded.battery_charge_milli(battery), Some(123_456));
    let mut power = PowerSystem::new();
    let update = power.update(&loaded);
    power.distribute(&mut loaded, 0.5, Duration::from_secs(1));
    assert!(update.topology_rebuilt);
    assert_eq!(update.connection_count, 3);
    assert!(power.is_powered(bore));
}

#[test]
fn machine_health_round_trips_after_damage() {
    let path = temp_world_path("machine-health");
    let mut world = World::empty(12, 16, 9).unwrap();
    for x in [3, 6] {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(3, 7))
        .unwrap();
    assert_eq!(world.damage_machine(bore, 37).applied, 37);
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    let health = loaded.machine_health(bore).unwrap();
    assert_eq!(health.current(), crate::DEFAULT_MACHINE_HEALTH - 37);
    assert_eq!(health.maximum(), crate::DEFAULT_MACHINE_HEALTH);
}

#[test]
fn composite_assembler_inventory_round_trips() {
    let path = temp_world_path("composite-assembler");
    let mut world = World::empty(12, 16, 9).unwrap();
    for x in 3..=5 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let assembler = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(3, 8))
        .unwrap();
    assert!(world.set_furniture_active(assembler, true));
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 4));
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(1, ItemStack::new(ItemId::STONE_BLOCK, 3));
    world
        .container_mut(assembler)
        .unwrap()
        .set_slot(2, ItemStack::new(ItemId::HARDENED_COMPOSITE, 2));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert!(loaded.object(assembler).unwrap().is_active());
    assert_eq!(loaded.container(assembler).unwrap().slots().len(), 3);
    assert_eq!(
        loaded.container(assembler).unwrap().slot(2),
        ItemStack::new(ItemId::HARDENED_COMPOSITE, 2)
    );
}

#[test]
fn red_shaft_bore_round_trips_with_storage_and_activation() {
    let path = temp_world_path("red-shaft-bore");
    let mut world = World::empty(16, 24, 9).unwrap();
    for x in [3, 8] {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bore = world
        .place_furniture(FurnitureObject::RED_SHAFT_BORE, TilePos::new(3, 7))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    world
        .container_mut(bore)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 17));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert!(loaded.object(bore).unwrap().is_active());
    assert_eq!(loaded.object(bore).unwrap().size(), [6, 3]);
    assert_eq!(
        loaded.container(bore).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 17)
    );
    assert_eq!(
        loaded.machine_health(bore).unwrap().current(),
        crate::DEFAULT_MACHINE_HEALTH
    );
}

#[test]
fn laser_drill_round_trips_with_aim_storage_and_activation() {
    let path = temp_world_path("laser-drill");
    let mut world = World::empty(32, 32, 9).unwrap();
    for x in [12, 14] {
        world
            .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let drill = world
        .place_furniture(FurnitureObject::LASER_DRILL, TilePos::new(12, 10))
        .unwrap();
    assert!(world.set_laser_drill_aim(drill, crate::LaserDrillAim::FarRight));
    assert!(world.set_furniture_active(drill, true));
    world
        .container_mut(drill)
        .unwrap()
        .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 17));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert!(loaded.object(drill).unwrap().is_active());
    assert_eq!(
        loaded.laser_drill_aim(drill),
        Some(crate::LaserDrillAim::FarRight)
    );
    assert_eq!(
        loaded.container(drill).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 17)
    );
}

#[test]
fn version_eleven_machines_migrate_with_full_health() {
    let path = temp_world_path("version-eleven-health");
    let mut world = World::empty(12, 16, 9).unwrap();
    for x in [3, 6] {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(3, 7))
        .unwrap();
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, world.name().len());
    bytes[8..10].copy_from_slice(&LIFT_STATION_VERSION.to_le_bytes());
    let mut cursor = HEADER_SIZE + world.name().len() + SESSION_METADATA_SIZE;
    for _ in 0..world.chunk_count() {
        let foreground_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        let background_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        cursor += 8 + foreground_len + background_len;
    }
    let payload_length_offset = cursor;
    let payload_length = read_u32(&bytes, &mut cursor).unwrap() as usize;
    cursor += 4;
    let payload_start = cursor;
    let object_count = u32::from_le_bytes(
        bytes[payload_start + 24..payload_start + 28]
            .try_into()
            .unwrap(),
    ) as usize;
    for index in (0..object_count).rev() {
        let record = payload_start + OBJECT_HEADER_SIZE + index * OBJECT_RECORD_SIZE;
        bytes.drain(record + MOTION_OBJECT_RECORD_SIZE..record + OBJECT_RECORD_SIZE);
    }
    let migrated_length =
        payload_length - object_count * (OBJECT_RECORD_SIZE - MOTION_OBJECT_RECORD_SIZE);
    bytes[payload_length_offset..payload_length_offset + 4]
        .copy_from_slice(&(migrated_length as u32).to_le_bytes());
    let migrated_checksum = checksum(&bytes[payload_start..payload_start + migrated_length]);
    bytes[payload_length_offset + 4..payload_length_offset + 8]
        .copy_from_slice(&migrated_checksum.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(
        loaded.machine_health(bore).unwrap().current(),
        crate::DEFAULT_MACHINE_HEALTH
    );
}

#[test]
fn background_supported_power_connector_round_trips() {
    let path = temp_world_path("power-connector");
    let mut world = World::empty(12, 16, 0).unwrap();
    world
        .set_tile(5, 6, Layer::Background, BackgroundTile::STONE_WALL)
        .unwrap();
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(5, 6))
        .unwrap();
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(
        loaded.object(connector).unwrap().object_type(),
        FurnitureObject::POWER_CONNECTOR
    );
    assert_eq!(loaded.object(connector).unwrap().root(), TilePos::new(5, 6));
}

#[test]
fn version_seven_batteries_migrate_empty() {
    let path = temp_world_path("version-seven-battery");
    let mut world = World::empty(8, 8, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let battery = world
        .place_furniture(FurnitureObject::BATTERY, TilePos::new(2, 4))
        .unwrap();
    assert!(world.set_battery_charge_milli(battery, 100_000));
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, world.name().len());
    bytes[8..10].copy_from_slice(&ACTIVATION_VERSION.to_le_bytes());
    let mut cursor = HEADER_SIZE + SESSION_METADATA_SIZE;
    for _ in 0..world.chunk_count() {
        let foreground_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        let background_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        cursor += 8 + foreground_len + background_len;
    }
    let payload_length_offset = cursor;
    let payload_length = read_u32(&bytes, &mut cursor).unwrap() as usize;
    cursor += 4;
    let payload_start = cursor;
    let object_count = u32::from_le_bytes(
        bytes[payload_start + 24..payload_start + 28]
            .try_into()
            .unwrap(),
    ) as usize;
    for index in (0..object_count).rev() {
        let record = payload_start + OBJECT_HEADER_SIZE + index * OBJECT_RECORD_SIZE;
        bytes.drain(record + ACTIVATION_OBJECT_RECORD_SIZE..record + OBJECT_RECORD_SIZE);
    }
    let migrated_length =
        payload_length - object_count * (OBJECT_RECORD_SIZE - ACTIVATION_OBJECT_RECORD_SIZE);
    bytes[payload_length_offset..payload_length_offset + 4]
        .copy_from_slice(&(migrated_length as u32).to_le_bytes());
    let migrated_checksum = checksum(&bytes[payload_start..payload_start + migrated_length]);
    bytes[payload_length_offset + 4..payload_length_offset + 8]
        .copy_from_slice(&migrated_checksum.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.battery_charge_milli(battery), Some(0));
}

#[test]
fn version_eight_turrets_migrate_with_zero_kills() {
    let path = temp_world_path("version-eight-turret");
    let mut world = World::empty(8, 12, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let turret = world
        .place_furniture(FurnitureObject::TURRET, TilePos::new(2, 6))
        .unwrap();
    assert!(world.increment_turret_kill_count(turret));
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, world.name().len());
    bytes[8..10].copy_from_slice(&BATTERY_STORAGE_VERSION.to_le_bytes());
    let mut cursor = HEADER_SIZE + world.name().len() + SESSION_METADATA_SIZE;
    for _ in 0..world.chunk_count() {
        let foreground_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        let background_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        cursor += 8 + foreground_len + background_len;
    }
    let payload_length_offset = cursor;
    let payload_length = read_u32(&bytes, &mut cursor).unwrap() as usize;
    cursor += 4;
    let payload_start = cursor;
    let object_count = u32::from_le_bytes(
        bytes[payload_start + 24..payload_start + 28]
            .try_into()
            .unwrap(),
    ) as usize;
    for index in (0..object_count).rev() {
        let record = payload_start + OBJECT_HEADER_SIZE + index * OBJECT_RECORD_SIZE;
        bytes.drain(record + BATTERY_OBJECT_RECORD_SIZE..record + OBJECT_RECORD_SIZE);
    }
    let migrated_length =
        payload_length - object_count * (OBJECT_RECORD_SIZE - BATTERY_OBJECT_RECORD_SIZE);
    bytes[payload_length_offset..payload_length_offset + 4]
        .copy_from_slice(&(migrated_length as u32).to_le_bytes());
    let migrated_checksum = checksum(&bytes[payload_start..payload_start + migrated_length]);
    bytes[payload_length_offset + 4..payload_length_offset + 8]
        .copy_from_slice(&migrated_checksum.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.turret_kill_count(turret), Some(0));
}

#[test]
fn version_nine_objects_migrate_without_motion_state() {
    let path = temp_world_path("version-nine-statistics");
    let mut world = World::empty(8, 12, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let turret = world
        .place_furniture(FurnitureObject::TURRET, TilePos::new(2, 6))
        .unwrap();
    assert!(world.increment_turret_kill_count(turret));
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, world.name().len());
    bytes[8..10].copy_from_slice(&STATISTICS_VERSION.to_le_bytes());
    let mut cursor = HEADER_SIZE + world.name().len() + SESSION_METADATA_SIZE;
    for _ in 0..world.chunk_count() {
        let foreground_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        let background_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        cursor += 8 + foreground_len + background_len;
    }
    let payload_length_offset = cursor;
    let payload_length = read_u32(&bytes, &mut cursor).unwrap() as usize;
    cursor += 4;
    let payload_start = cursor;
    let object_count = u32::from_le_bytes(
        bytes[payload_start + 24..payload_start + 28]
            .try_into()
            .unwrap(),
    ) as usize;
    for index in (0..object_count).rev() {
        let record = payload_start + OBJECT_HEADER_SIZE + index * OBJECT_RECORD_SIZE;
        bytes.drain(record + STATISTICS_OBJECT_RECORD_SIZE..record + OBJECT_RECORD_SIZE);
    }
    let migrated_length =
        payload_length - object_count * (OBJECT_RECORD_SIZE - STATISTICS_OBJECT_RECORD_SIZE);
    bytes[payload_length_offset..payload_length_offset + 4]
        .copy_from_slice(&(migrated_length as u32).to_le_bytes());
    let migrated_checksum = checksum(&bytes[payload_start..payload_start + migrated_length]);
    bytes[payload_length_offset + 4..payload_length_offset + 8]
        .copy_from_slice(&migrated_checksum.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.turret_kill_count(turret), Some(1));
    assert_eq!(loaded.object(turret).unwrap().linked_object(), None);
}

#[test]
fn every_container_type_round_trips_together_with_all_slots() {
    let path = temp_world_path("all-containers");
    let mut world = World::empty(24, 40, 11).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
    }
    for x in [10, 13] {
        world
            .set_tile(x, 9, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in 16..=18 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 4))
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(10, 6))
        .unwrap();
    let launcher = world
        .place_furniture(
            FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
            TilePos::new(16, 7),
        )
        .unwrap();
    let conveyor = world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(19, 9))
        .unwrap();
    assert!(
        world
            .container_mut(chest)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 713))
    );
    assert!(
        world
            .container_mut(chest)
            .unwrap()
            .set_slot(39, ItemStack::new(ItemId::HEALING_POTION, 17))
    );
    assert!(
        world
            .container_mut(bore)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 456))
    );
    assert!(
        world
            .container_mut(bore)
            .unwrap()
            .set_slot(9, ItemStack::new(ItemId::RED_LIGHT, 23))
    );
    assert!(
        world
            .container_mut(launcher)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 99))
    );
    assert!(
        world
            .container_mut(launcher)
            .unwrap()
            .set_slot(7, ItemStack::new(ItemId::STONE_BLOCK, 51))
    );
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.container(chest), world.container(chest));
    assert_eq!(loaded.container(bore), world.container(bore));
    assert_eq!(loaded.container(launcher), world.container(launcher));
    assert_eq!(
        loaded.object_at(TilePos::new(19, 9)).map(WorldObject::id),
        Some(conveyor)
    );
    assert_eq!(loaded.object(conveyor).unwrap().root(), TilePos::new(19, 9));
    assert!(loaded.container(conveyor).is_none());
    assert_eq!(loaded.container(chest).unwrap().slots().len(), 40);
    assert_eq!(
        loaded.container(bore).unwrap().slots().len(),
        usize::from(crate::LASER_BORE_SLOTS)
    );
    assert_eq!(
        loaded.container(launcher).unwrap().slots().len(),
        usize::from(crate::ORBITAL_EXPORT_LAUNCHER_SLOTS)
    );
}

#[test]
fn every_built_in_inventory_type_has_round_trip_coverage() {
    let actual: Vec<_> = crate::BUILT_IN_FURNITURE
        .iter()
        .filter(|definition| definition.interaction().container_slots().is_some())
        .map(|definition| definition.object_type())
        .collect();
    let expected = vec![
        FurnitureObject::CHEST,
        FurnitureObject::LASER_BORE,
        FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
        FurnitureObject::CARGO_LIFT,
        FurnitureObject::LIFT_STATION,
        FurnitureObject::COMPOSITE_ASSEMBLER,
        FurnitureObject::RED_SHAFT_BORE,
        FurnitureObject::LASER_DRILL,
        FurnitureObject::AMMO_TURRET,
    ];
    assert_eq!(
        actual, expected,
        "add persistence coverage for new inventory furniture"
    );
}

#[test]
fn laser_bore_round_trips_with_its_mining_progress_and_schedule() {
    let path = temp_world_path("laser-bore");
    let mut world = World::empty(16, 80, 4).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in [9, 11, 12] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(5, 12, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let id = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(9, 6))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(11, 5))
        .unwrap();
    assert!(world.set_furniture_active(id, true));
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, Duration::from_secs(1));
    world.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    world.save(&path).unwrap();

    let mut loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.object(id).unwrap().size(), [4, 3]);
    assert!(loaded.object(id).unwrap().is_active());
    let mut power = PowerSystem::new();
    power.distribute(&mut loaded, 0.5, Duration::from_secs(1));
    for _ in 0..2 {
        loaded.update_decorations_with_power(Duration::from_secs(1), 8, &power);
    }
    assert_eq!(
        loaded.tile(5, 12, Layer::Foreground).unwrap(),
        TileId::EMPTY
    );
    assert_eq!(
        loaded.container(id).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 1)
    );
    assert!(loaded.object(id).is_some());
}

#[test]
fn turret_activation_and_target_priority_round_trip_without_storage() {
    let path = temp_world_path("turret");
    let mut world = World::empty(12, 16, 4).unwrap();
    for x in 3..=4 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let turret = world
        .place_furniture_facing(
            FurnitureObject::TURRET,
            TilePos::new(3, 6),
            crate::FurnitureFacing::Left,
        )
        .unwrap();
    assert!(world.set_furniture_target_priority(turret, TargetPriority::Furthest));
    assert!(world.set_furniture_active(turret, true));
    assert!(world.increment_turret_kill_count(turret));
    assert!(world.increment_turret_kill_count(turret));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert!(loaded.object(turret).unwrap().is_active());
    assert_eq!(
        loaded.furniture_target_priority(turret),
        Some(TargetPriority::Furthest)
    );
    assert_eq!(loaded.turret_kill_count(turret), Some(2));
    assert_eq!(
        loaded.furniture_facing(turret),
        Some(crate::FurnitureFacing::Left)
    );
    assert!(loaded.container(turret).is_none());
}

#[test]
fn ammunition_and_directional_turrets_round_trip_with_facing_and_ammo() {
    let path = temp_world_path("new-turrets");
    let mut world = World::empty(16, 16, 4).unwrap();
    for x in 3..=4 {
        world
            .set_tile(x, 9, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(10, 7, Layer::Background, BackgroundTile::STONE_WALL)
        .unwrap();
    let ammunition = world
        .place_furniture_facing(
            FurnitureObject::AMMO_TURRET,
            TilePos::new(3, 7),
            crate::FurnitureFacing::Left,
        )
        .unwrap();
    assert!(world.set_furniture_active(ammunition, true));
    assert!(world.set_furniture_target_priority(ammunition, TargetPriority::Weakest));
    assert!(
        world
            .container_mut(ammunition)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::TURRET_AMMO, 37))
    );
    let sentry = world
        .place_furniture_facing(
            FurnitureObject::DIRECTIONAL_SENTRY,
            TilePos::new(10, 7),
            crate::FurnitureFacing::Right,
        )
        .unwrap();
    assert!(world.set_furniture_active(sentry, true));
    assert!(world.increment_turret_kill_count(sentry));
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(
        loaded.furniture_facing(ammunition),
        Some(crate::FurnitureFacing::Left)
    );
    assert_eq!(
        loaded.furniture_target_priority(ammunition),
        Some(TargetPriority::Weakest)
    );
    assert_eq!(
        loaded.container(ammunition).unwrap().slot(0),
        ItemStack::new(ItemId::TURRET_AMMO, 37)
    );
    assert_eq!(
        loaded.furniture_facing(sentry),
        Some(crate::FurnitureFacing::Right)
    );
    assert!(loaded.object(sentry).unwrap().is_active());
    assert_eq!(loaded.turret_kill_count(sentry), Some(1));
}

#[test]
fn furniture_supported_by_a_directional_sentry_round_trips() {
    let path = temp_world_path("structural-sentry");
    let mut world = World::empty(12, 12, 17).unwrap();
    world
        .set_tile(6, 6, Layer::Background, BackgroundTile::STONE_WALL)
        .unwrap();
    let sentry = world
        .place_furniture_facing(
            FurnitureObject::DIRECTIONAL_SENTRY,
            TilePos::new(6, 6),
            crate::FurnitureFacing::Left,
        )
        .unwrap();
    let spikes = world
        .place_furniture(FurnitureObject::SPIKES, TilePos::new(6, 5))
        .unwrap();
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.object(spikes).unwrap().root(), TilePos::new(6, 6));
    assert_eq!(
        loaded.furniture_facing(sentry),
        Some(crate::FurnitureFacing::Left)
    );
    assert!(!loaded.can_remove_object(sentry));
}

#[test]
fn rope_column_round_trips_as_one_persistent_object() {
    let path = temp_world_path("rope-column");
    let mut world = World::empty(8, 16, 4).unwrap();
    world
        .set_tile(3, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world.place_or_extend_rope(TilePos::new(3, 3)).unwrap();
    world.place_or_extend_rope(TilePos::new(3, 3)).unwrap();
    world.place_or_extend_rope(TilePos::new(3, 3)).unwrap();
    let rope = world.object_at(TilePos::new(3, 4)).unwrap().id();
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(
        loaded.object(rope).unwrap().object_type(),
        crate::ROPE_OBJECT
    );
    assert_eq!(loaded.object(rope).unwrap().size(), [1, 3]);
    assert_eq!(loaded.object_at(TilePos::new(3, 5)).unwrap().id(), rope);
}

#[test]
fn background_anchored_rope_round_trips() {
    let path = temp_world_path("background-anchored-rope");
    let mut world = World::empty(8, 16, 4).unwrap();
    let anchor = TilePos::new(3, 3);
    world
        .set_tile(
            anchor.x,
            anchor.y,
            Layer::Background,
            crate::BackgroundTile::STONE_WALL,
        )
        .unwrap();
    world.place_or_extend_rope(anchor).unwrap();
    world.place_or_extend_rope(anchor).unwrap();
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    let rope = loaded.object_at(anchor).unwrap();
    assert_eq!(rope.object_type(), crate::ROPE_OBJECT);
    assert_eq!(rope.root(), anchor);
    assert_eq!(rope.size(), [1, 2]);
}

#[test]
fn moving_cargo_lift_round_trips_with_track_and_inventory() {
    let path = temp_world_path("moving-cargo-lift");
    let mut world = World::empty(24, 24, 4).unwrap();
    world
        .set_tile(10, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    for _ in 0..8 {
        world
            .place_or_extend_powered_cable(TilePos::new(10, 3))
            .unwrap();
    }
    let cable = world.object_at(TilePos::new(10, 5)).unwrap().id();
    let lift = world.place_cargo_lift(TilePos::new(10, 4)).unwrap();
    assert!(
        world
            .container_mut(lift)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 17))
    );

    for x in 2..=3 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(6, 7, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 5))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 5))
        .unwrap();
    let mut power = PowerSystem::new();
    assert!(world.set_cargo_lift_direction(lift, crate::CargoLiftDirection::Down));
    power.distribute(&mut world, 0.5, Duration::from_millis(100));
    assert_eq!(
        world.update_cargo_lifts(
            Duration::from_millis(100),
            &power,
            &crate::ItemRegistry::with_built_ins(),
        ),
        1
    );
    assert_eq!(world.object(lift).unwrap().motion_position_tiles(), 4.6);
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded.cargo_lift_cable(lift), Some(cable));
    assert_eq!(
        loaded.cargo_lift_direction(lift),
        Some(crate::CargoLiftDirection::Down)
    );
    assert_eq!(loaded.object(lift).unwrap().anchor(), TilePos::new(11, 5));
    assert_eq!(loaded.object(lift).unwrap().motion_position_tiles(), 4.6);
    assert_eq!(
        loaded.container(lift).unwrap().slot(0),
        ItemStack::new(ItemId::STONE_BLOCK, 17)
    );
    assert_eq!(loaded.object(cable).unwrap().root(), TilePos::new(10, 2));
}

#[test]
fn lift_station_round_trips_with_configuration_inventory_and_cable_index() {
    let path = temp_world_path("lift-station");
    let mut world = World::empty(24, 24, 4).unwrap();
    world
        .set_tile(10, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(10, 3))
        .unwrap();
    for _ in 0..10 {
        world
            .place_or_extend_powered_cable(TilePos::new(10, 3))
            .unwrap();
    }
    let cable = world.object_at(TilePos::new(10, 6)).unwrap().id();
    world.place_cargo_lift(TilePos::new(10, 4)).unwrap();
    for x in 8..=9 {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let station = world.place_lift_station(TilePos::new(8, 8)).unwrap();
    assert!(world.set_lift_station_mode(station, crate::LiftStationMode::Unload));
    assert!(world.set_lift_station_departure(station, crate::CargoLiftDirection::Up));
    assert!(
        world
            .container_mut(station)
            .unwrap()
            .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 41))
    );
    world.save(&path).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    let configuration = loaded.lift_station_configuration(station).unwrap();
    assert_eq!(configuration.mode(), crate::LiftStationMode::Unload);
    assert_eq!(configuration.departure(), crate::CargoLiftDirection::Up);
    assert_eq!(loaded.object(station).unwrap().linked_object(), Some(cable));
    assert_eq!(
        loaded.container(station).unwrap().slot(0),
        ItemStack::new(ItemId::DIRT_BLOCK, 41)
    );
    assert!(!loaded.can_remove_object(cable));
    assert!(
        loaded
            .lift_station_placement_target(TilePos::new(8, 8))
            .is_err()
    );
}

#[test]
fn version_ten_motion_saves_remain_loadable() {
    let path = temp_world_path("version-ten-motion");
    let world = World::empty(8, 8, 7).unwrap();
    world.save(&path).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, world.name().len());
    bytes[8..10].copy_from_slice(&MOTION_VERSION.to_le_bytes());
    fs::write(&path, bytes).unwrap();

    assert!(World::read_name(&path).is_ok());
    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(loaded, world);
}

#[test]
fn version_five_laser_bores_migrate_with_empty_storage() {
    let path = temp_world_path("version-five-laser-bore");
    let mut world = World::empty(16, 32, 4).unwrap();
    for x in [4, 7] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(4, 5))
        .unwrap();
    world.save(&path).unwrap();

    let mut bytes = fs::read(&path).unwrap();
    remove_empty_specialist_section(&mut bytes);
    remove_player_state(&mut bytes, world.name().len());
    bytes[8..10].copy_from_slice(&SESSION_METADATA_VERSION.to_le_bytes());
    let mut cursor = HEADER_SIZE + world.name().len() + SESSION_METADATA_SIZE;
    for _ in 0..world.chunk_count() {
        let foreground_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        let background_len = read_u32(&bytes, &mut cursor).unwrap() as usize;
        cursor += 8 + foreground_len + background_len;
    }
    let payload_length_offset = cursor;
    let _payload_length = read_u32(&bytes, &mut cursor).unwrap() as usize;
    let _payload_checksum = read_u32(&bytes, &mut cursor).unwrap();
    let payload_start = cursor;
    let object_count = u32::from_le_bytes(
        bytes[payload_start + 24..payload_start + 28]
            .try_into()
            .unwrap(),
    ) as usize;
    assert_eq!(object_count, 1);
    remove_activation_flags(&mut bytes, payload_start, object_count);
    let legacy_payload_len = OBJECT_HEADER_SIZE + object_count * LEGACY_OBJECT_RECORD_SIZE;
    bytes[payload_start + 28..payload_start + 32].copy_from_slice(&0_u32.to_le_bytes());
    bytes[payload_length_offset..payload_length_offset + 4]
        .copy_from_slice(&(legacy_payload_len as u32).to_le_bytes());
    let payload_checksum = checksum(&bytes[payload_start..payload_start + legacy_payload_len]);
    bytes[payload_length_offset + 4..payload_length_offset + 8]
        .copy_from_slice(&payload_checksum.to_le_bytes());
    bytes.truncate(payload_start + legacy_payload_len);
    fs::write(&path, bytes).unwrap();

    let loaded = World::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(
        loaded.container(bore).unwrap().slots().len(),
        usize::from(crate::LASER_BORE_SLOTS)
    );
    assert!(loaded.container(bore).unwrap().is_empty());
    assert!(!loaded.object(bore).unwrap().is_active());
}
