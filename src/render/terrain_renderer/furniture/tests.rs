use super::*;
use crate::{ForegroundTile, FurnitureObject, Layer, TilePos};

#[test]
fn chest_instance_covers_its_two_by_two_world_footprint() {
    let mut world = World::empty(8, 8, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 5, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
    }
    let id = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].position, [2.5, -3.7, 0.25]);
    assert_eq!(instances[0].size, [2.0, 2.0]);
    assert_eq!(
        instances[0].uv_rect,
        atlas_frame_uv(0, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );
}

#[test]
fn cable_anchor_lift_and_station_use_procedural_furniture_visuals() {
    let mut world = World::empty(16, 20, 0).unwrap();
    world
        .set_tile(6, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let anchor = world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(6, 3))
        .unwrap();
    for _ in 0..6 {
        world
            .place_or_extend_powered_cable(TilePos::new(6, 3))
            .unwrap();
    }
    let lift = world.place_cargo_lift(TilePos::new(6, 4)).unwrap();
    for x in 4..=5 {
        world
            .set_tile(x, 9, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let station = world.place_lift_station(TilePos::new(4, 7)).unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(anchor).unwrap(), &mut instances);
    append_furniture_instance(&world, world.object(lift).unwrap(), &mut instances);
    append_furniture_instance(&world, world.object(station).unwrap(), &mut instances);

    assert_eq!(instances[0].visual_kind, 1);
    assert_eq!(instances[0].size, [1.0, 1.0]);
    assert_eq!(instances[1].visual_kind, 2);
    assert_eq!(instances[1].size, [2.0, 2.0]);
    assert_eq!(instances[1].position, [7.5, -4.5, 0.24]);
    assert_eq!(instances[2].visual_kind, 5);
    assert_eq!(instances[2].size, [2.0, 2.0]);
    assert_eq!(instances[2].position, [4.5, -7.7, 0.25]);
}

#[test]
fn laser_bore_uses_the_second_atlas_frame_and_emits_to_its_target() {
    let mut world = World::empty(12, 20, 0).unwrap();
    for x in [2, 5] {
        world
            .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in [7, 9, 10] {
        world
            .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(3, 10, Layer::Foreground, ForegroundTile::DIRT)
        .unwrap();
    let id = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 4))
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(7, 5))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(9, 4))
        .unwrap();
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, std::time::Duration::from_secs(1));
    assert!(world.set_furniture_active(id, true));
    let object = world.object(id).unwrap();

    let mut furniture = Vec::new();
    append_furniture_instance(&world, object, &mut furniture);
    assert_eq!(furniture[0].position, [3.5, -5.2, 0.25]);
    assert_eq!(furniture[0].size, [4.0, 3.0]);
    assert_eq!(
        furniture[0].uv_rect,
        atlas_frame_uv(1, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );

    let mut beams = Vec::new();
    let mut lights = Vec::new();
    let mut emitters = Vec::new();
    append_laser_bore(
        &world,
        &power,
        object,
        &mut beams,
        &mut lights,
        &mut emitters,
    );
    assert_eq!(beams.len(), 3);
    assert!(beams.iter().all(|beam| beam.beam_kind == 0));
    assert!(beams.iter().all(|beam| beam.position[0] == 3.5));
    assert!(beams.iter().all(|beam| beam.position[2] == 0.30));
    assert!(
        beams
            .iter()
            .all(|beam| beam.size[0] == LASER_BORE_BEAM_WIDTH && beam.size[1] <= 1.0)
    );
    assert!((beams.iter().map(|beam| beam.size[1]).sum::<f32>() - 2.8).abs() < 0.000_01);
    assert_eq!(lights.len(), 3);
    assert_eq!(lights[0].position, [3.5, 7.0]);
    assert_eq!(lights[2].position, [3.5, 9.0]);
    assert!(
        lights
            .iter()
            .all(|light| light.kind == FurnitureLightKind::Laser)
    );
    assert_eq!(emitters.len(), 1);
    assert_eq!(emitters[0].impact, Some([3.5, 9.5]));
    assert_eq!(emitters[0].width, 2.0);
}

#[test]
fn red_shaft_bore_renders_one_wide_red_beam() {
    let mut world = World::empty(24, 20, 0).unwrap();
    for x in [2, 5, 6, 10, 15, 18, 19] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    for x in 11..=14 {
        world
            .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(2, 6))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(5, 5))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(18, 5))
        .unwrap();
    let bore = world
        .place_furniture(FurnitureObject::RED_SHAFT_BORE, TilePos::new(10, 5))
        .unwrap();
    assert!(world.set_furniture_active(bore, true));
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, std::time::Duration::from_secs(1));
    let object = world.object(bore).unwrap();
    let mut furniture = Vec::new();
    append_furniture_instance(&world, object, &mut furniture);
    let mut beams = Vec::new();
    let mut lights = Vec::new();
    let mut emitters = Vec::new();
    append_laser_bore(
        &world,
        &power,
        object,
        &mut beams,
        &mut lights,
        &mut emitters,
    );

    assert_eq!(furniture[0].visual_kind, 8);
    assert_eq!(furniture[0].size, [6.0, 3.0]);
    assert_eq!(beams.len(), 4);
    assert!(beams.iter().all(|beam| beam.beam_kind == 1));
    assert!(beams.iter().all(|beam| beam.position[0] == 12.5));
    assert!(
        beams
            .iter()
            .all(|beam| beam.size[0] == 4.0 && beam.size[1] <= 1.0)
    );
    assert!(
        lights
            .iter()
            .all(|light| light.kind == FurnitureLightKind::RedLaser)
    );
    assert_eq!(emitters.len(), 1);
    assert_eq!(emitters[0].impact, Some([12.5, 11.5]));
    assert_eq!(emitters[0].width, 4.0);
    assert_eq!(emitters[0].kind, LaserParticleKind::Red);
}

#[test]
fn aimed_laser_drill_rotates_its_sprite_beam_and_machine_head() {
    let mut world = World::empty(64, 32, 0).unwrap();
    for x in [16, 17, 20, 30, 32] {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(35, 14, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(20, 8))
        .unwrap();
    world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(16, 7))
        .unwrap();
    let drill = world
        .place_furniture(FurnitureObject::LASER_DRILL, TilePos::new(30, 8))
        .unwrap();
    assert!(world.set_laser_drill_aim(drill, crate::LaserDrillAim::Right));
    assert!(world.set_furniture_active(drill, true));
    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, std::time::Duration::from_secs(1));
    let object = world.object(drill).unwrap();
    let mut furniture = Vec::new();
    append_furniture_instance(&world, object, &mut furniture);
    let mut beams = Vec::new();
    let mut lights = Vec::new();
    let mut emitters = Vec::new();
    append_laser_bore(
        &world,
        &power,
        object,
        &mut beams,
        &mut lights,
        &mut emitters,
    );

    assert_eq!(furniture[0].visual_kind, 15);
    assert!(beams.len() > 1);
    assert!(beams.iter().all(|beam| beam.beam_kind == 0));
    assert!(beams.iter().all(|beam| beam.direction[0] > 0.0));
    assert!(beams.iter().all(|beam| beam.direction[1] < 0.0));
    assert!(beams.iter().all(|beam| beam.size[1] <= 1.000_01));
    assert_eq!(emitters[0].impact, Some([35.0, 13.5]));
}

#[test]
fn turret_uses_the_third_atlas_frame() {
    let mut world = World::empty(8, 8, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 5, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let id = world
        .place_furniture(FurnitureObject::TURRET, TilePos::new(2, 3))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);
    assert_eq!(instances[0].size, [2.0, 2.0]);
    assert_eq!(
        instances[0].uv_rect,
        atlas_frame_uv(2, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );
}

#[test]
fn turret_visuals_follow_their_persisted_facing() {
    let mut world = World::empty(16, 12, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .set_tile(8, 5, Layer::Background, crate::BackgroundTile::STONE_WALL)
        .unwrap();
    let turret = world
        .place_furniture_facing(
            FurnitureObject::TURRET,
            TilePos::new(2, 5),
            crate::FurnitureFacing::Left,
        )
        .unwrap();
    let sentry = world
        .place_furniture_facing(
            FurnitureObject::DIRECTIONAL_SENTRY,
            TilePos::new(8, 5),
            crate::FurnitureFacing::Right,
        )
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(turret).unwrap(), &mut instances);
    append_furniture_instance(&world, world.object(sentry).unwrap(), &mut instances);

    assert!(instances[0].uv_rect[2] < 0.0);
    assert_eq!(instances[1].visual_kind, 19);
    assert_eq!(instances[1].size, [1.0, 1.0]);
}

#[test]
fn ammunition_turret_uses_its_directional_procedural_visual() {
    let mut world = World::empty(12, 12, 0).unwrap();
    for x in 3..=4 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let turret = world
        .place_furniture_facing(
            FurnitureObject::AMMO_TURRET,
            TilePos::new(3, 6),
            crate::FurnitureFacing::Left,
        )
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(turret).unwrap(), &mut instances);

    assert_eq!(instances[0].visual_kind, 18);
    assert_eq!(instances[0].size, [2.0, 2.0]);
}

#[test]
fn spikes_use_a_single_tile_procedural_visual() {
    let mut world = World::empty(8, 8, 0).unwrap();
    world
        .set_tile(3, 6, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let spikes = world
        .place_furniture(FurnitureObject::SPIKES, TilePos::new(3, 5))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(spikes).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].visual_kind, 21);
    assert_eq!(instances[0].size, [1.0, 1.0]);
}

#[test]
fn orbital_export_launcher_uses_the_fourth_atlas_frame() {
    let mut world = World::empty(10, 10, 0).unwrap();
    for x in 2..=4 {
        world
            .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let id = world
        .place_furniture(FurnitureObject::ORBITAL_EXPORT_LAUNCHER, TilePos::new(2, 4))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);
    assert_eq!(instances[0].size, [3.0, 3.0]);
    assert_eq!(
        instances[0].uv_rect,
        atlas_frame_uv(3, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );
}

#[test]
fn cargo_conveyor_uses_a_connected_atlas_frame() {
    let mut world = World::empty(10, 10, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 5))
        .unwrap();
    let id = world
        .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(4, 6))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);
    assert_eq!(instances[0].position, [4.0, -6.0, 0.25]);
    assert_eq!(instances[0].size, [1.0, 1.0]);
    assert_eq!(
        instances[0].uv_rect,
        atlas_frame_uv(4, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );
}

#[test]
fn power_furniture_uses_the_appended_atlas_frames() {
    let mut world = World::empty(12, 12, 0).unwrap();
    for x in [2, 3, 6, 8, 9] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let solar = world
        .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 5))
        .unwrap();
    let pylon = world
        .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 6))
        .unwrap();
    let battery = world
        .place_furniture(FurnitureObject::BATTERY, TilePos::new(8, 6))
        .unwrap();

    let mut power = PowerSystem::new();
    power.distribute(&mut world, 0.5, std::time::Duration::from_secs(1));
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(solar).unwrap(), &mut instances);
    append_furniture_instance(&world, world.object(pylon).unwrap(), &mut instances);
    append_furniture_instance(&world, world.object(battery).unwrap(), &mut instances);
    assert_eq!(instances[0].size, [2.0, 3.0]);
    assert_eq!(
        instances[0].uv_rect,
        atlas_frame_uv(10, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );
    assert_eq!(instances[1].size, [1.0, 2.0]);
    assert_eq!(
        instances[1].uv_rect,
        atlas_frame_uv(11, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );
    assert_eq!(instances[2].size, [2.0, 2.0]);
    assert_eq!(
        instances[2].uv_rect,
        atlas_frame_uv(12, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
    );
    let mut indicator_lights = Vec::new();
    append_power_indicator_light(&power, world.object(pylon).unwrap(), &mut indicator_lights);
    append_power_indicator_light(
        &power,
        world.object(battery).unwrap(),
        &mut indicator_lights,
    );
    assert_eq!(indicator_lights.len(), 2);
    assert_eq!(indicator_lights[0].kind, FurnitureLightKind::Pylon);
    assert_eq!(indicator_lights[1].kind, FurnitureLightKind::Battery);

    assert!(world.remove_object(solar).is_some());
    power.distribute(&mut world, 0.9, std::time::Duration::from_secs(1));
    indicator_lights.clear();
    append_power_indicator_light(&power, world.object(pylon).unwrap(), &mut indicator_lights);
    append_power_indicator_light(
        &power,
        world.object(battery).unwrap(),
        &mut indicator_lights,
    );
    assert_eq!(indicator_lights.len(), 2);

    assert!(world.set_battery_charge_milli(battery, 0));
    power.distribute(&mut world, 0.9, std::time::Duration::from_secs(1));
    indicator_lights.clear();
    append_power_indicator_light(&power, world.object(pylon).unwrap(), &mut indicator_lights);
    append_power_indicator_light(
        &power,
        world.object(battery).unwrap(),
        &mut indicator_lights,
    );
    assert!(indicator_lights.is_empty());
}

#[test]
fn power_connector_uses_a_procedural_one_tile_visual() {
    let mut world = World::empty(8, 8, 0).unwrap();
    world
        .set_tile(
            3,
            3,
            crate::Layer::Background,
            crate::BackgroundTile::STONE_WALL,
        )
        .unwrap();
    let connector = world
        .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(3, 3))
        .unwrap();
    let mut instances = Vec::new();

    append_furniture_instance(&world, world.object(connector).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].size, [1.0, 1.0]);
    assert_eq!(instances[0].visual_kind, 6);
}

#[test]
fn composite_assembler_uses_a_procedural_three_by_two_visual() {
    let mut world = World::empty(8, 8, 1).unwrap();
    for x in 2..=4 {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let assembler = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(2, 4))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(assembler).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].visual_kind, 7);
    assert_eq!(instances[0].size, [3.0, 2.0]);
}

#[test]
fn procurement_terminal_uses_a_procedural_two_by_two_visual() {
    let mut world = World::empty(8, 8, 1).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let terminal = world
        .place_furniture(FurnitureObject::PROCUREMENT_TERMINAL, TilePos::new(2, 4))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(terminal).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].visual_kind, 9);
    assert_eq!(instances[0].size, [2.0, 2.0]);
}

#[test]
fn settlement_door_uses_a_procedural_one_by_three_visual() {
    let mut world = World::empty(8, 8, 1).unwrap();
    world
        .set_tile(3, 6, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let door = world
        .place_furniture(FurnitureObject::DOOR, TilePos::new(3, 3))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(door).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].visual_kind, 22);
    assert_eq!(instances[0].size, [1.0, 3.0]);
}

#[test]
fn settlement_bed_uses_a_procedural_two_by_one_visual() {
    let mut world = World::empty(8, 8, 1).unwrap();
    for x in 3..=4 {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bed = world
        .place_furniture(FurnitureObject::BED, TilePos::new(3, 5))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(bed).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].visual_kind, 24);
    assert_eq!(instances[0].size, [2.0, 1.0]);
}

#[test]
fn subsurface_surveyor_uses_a_procedural_three_by_two_visual() {
    let mut world = World::empty(10, 8, 1).unwrap();
    for x in 3..=5 {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let surveyor = world
        .place_furniture(FurnitureObject::SUBSURFACE_SURVEYOR, TilePos::new(3, 4))
        .unwrap();
    let mut instances = Vec::new();
    append_furniture_instance(&world, world.object(surveyor).unwrap(), &mut instances);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].visual_kind, 25);
    assert_eq!(instances[0].size, [3.0, 2.0]);
}

#[test]
fn cable_is_a_single_segment_batch_with_downward_sag() {
    let mut world = World::empty(32, 16, 0).unwrap();
    for x in [2, 10] {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(x, 6))
            .unwrap();
    }
    let mut power = PowerSystem::new();
    power.update(&world);
    let connection = power.connections()[0];
    let mut segments = Vec::new();
    append_power_cable(connection, &mut segments);

    assert_eq!(segments.len(), CABLE_SEGMENTS);
    let straight_y = (connection.start()[1] + connection.end()[1]) * 0.5;
    assert!(-segments[CABLE_SEGMENTS / 2].position[1] > straight_y);
    assert!(
        segments
            .iter()
            .all(|segment| segment.size[1] == CABLE_WIDTH)
    );
}

#[test]
fn furniture_light_flicker_is_subtle_smooth_and_bounded() {
    let phase = flicker_phase(TilePos::new(12, 34));
    let laser_a = laser_light_intensity(2.0, phase);
    let laser_b = laser_light_intensity(2.01, phase);
    let pylon_a = pylon_light_intensity(2.0, phase);
    let pylon_b = pylon_light_intensity(2.01, phase);
    let battery_a = battery_light_intensity(2.0, phase);
    let battery_b = battery_light_intensity(2.01, phase);

    assert!((0.82..=1.0).contains(&laser_a));
    assert!((0.72..=0.91).contains(&pylon_a));
    assert!((0.79..=0.85).contains(&battery_a));
    assert!((laser_b - laser_a).abs() < 0.02);
    assert!((pylon_b - pylon_a).abs() < 0.02);
    assert!((battery_b - battery_a).abs() < 0.01);
}
