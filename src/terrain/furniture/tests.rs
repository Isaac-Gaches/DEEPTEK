use super::*;

#[test]
fn built_in_furniture_ids_are_unique() {
    for (index, definition) in BUILT_IN_FURNITURE.iter().enumerate() {
        assert!(
            BUILT_IN_FURNITURE[index + 1..]
                .iter()
                .all(|other| other.object_type != definition.object_type)
        );
        assert_eq!(definition.object_type().raw(), 256 + index as u16);
        assert_eq!(
            furniture_definition(definition.object_type()),
            Some(*definition)
        );
    }
}

#[test]
fn cargo_roles_and_connector_are_definition_owned() {
    assert_eq!(
        CHEST_DEFINITION.interaction().item_transport_role(),
        Some(ItemTransportRole::Buffer)
    );
    assert_eq!(
        LASER_BORE_DEFINITION.interaction().item_transport_role(),
        Some(ItemTransportRole::Output)
    );
    assert_eq!(
        ORBITAL_EXPORT_LAUNCHER_DEFINITION
            .interaction()
            .item_transport_role(),
        Some(ItemTransportRole::Input)
    );
    assert!(CARGO_CONVEYOR_DEFINITION.is_item_transport_connector());
    assert_eq!(CARGO_CONVEYOR_DEFINITION.size(), [1, 1]);
    assert_eq!(
        LIFT_STATION_DEFINITION.interaction().item_transport_role(),
        Some(ItemTransportRole::Buffer)
    );
    assert!(
        LIFT_STATION_DEFINITION
            .interaction()
            .shows_lift_station_controls()
    );
    assert_eq!(
        CARGO_LIFT_DEFINITION.power_role(),
        Some(PowerRole::Consumer)
    );
    assert_eq!(
        COMPOSITE_ASSEMBLER_DEFINITION
            .interaction()
            .item_transport_role(),
        Some(ItemTransportRole::Processor)
    );
    assert_eq!(COMPOSITE_ASSEMBLER_DEFINITION.size(), [3, 2]);
    assert_eq!(
        RED_SHAFT_BORE_DEFINITION
            .interaction()
            .item_transport_role(),
        Some(ItemTransportRole::Output)
    );
    assert_eq!(
        LASER_DRILL_DEFINITION.interaction().item_transport_role(),
        Some(ItemTransportRole::Output)
    );
    assert_eq!(
        LASER_DRILL_DEFINITION.interaction().configuration(),
        Some(FurnitureConfiguration::LaserAim)
    );
    assert_eq!(
        AMMO_TURRET_DEFINITION.interaction().item_transport_role(),
        Some(ItemTransportRole::Input)
    );
    assert!(AMMO_TURRET_DEFINITION.supports_facing());
    assert!(DIRECTIONAL_SENTRY_DEFINITION.supports_facing());
    assert!(DIRECTIONAL_SENTRY_DEFINITION.is_structural());
    assert!(!AMMO_TURRET_DEFINITION.is_structural());
    assert_eq!(SPIKES_DEFINITION.size(), [1, 1]);
    assert_eq!(SPIKES_DEFINITION.support(), FurnitureSupport::Floor);
    assert_eq!(SPIKES_DEFINITION.chunk_activity(), ChunkActivity::None);
    assert_eq!(SPIKES_DEFINITION.maximum_health(), None);
    assert!(DOOR_DEFINITION.is_room_boundary());
    assert_eq!(DOOR_DEFINITION.size(), [1, 3]);
    assert!(BED_DEFINITION.interaction().allows_sleep());
    assert_eq!(BED_DEFINITION.size(), [2, 1]);
    assert_eq!(SUBSURFACE_SURVEYOR_DEFINITION.size(), [3, 2]);
    assert!(
        SUBSURFACE_SURVEYOR_DEFINITION
            .interaction()
            .shows_subsurface_survey()
    );
    assert_eq!(
        SUBSURFACE_SURVEYOR_DEFINITION.power_role(),
        Some(PowerRole::Consumer)
    );
    assert_eq!(
        SUBSURFACE_SURVEYOR_DEFINITION.power_rate_milli_per_second(),
        SUBSURFACE_SURVEYOR_DEMAND_MILLI_PER_SECOND
    );
    assert_eq!(LASER_BORE_DEFINITION.noise_emission(), 8);
    assert_eq!(RED_SHAFT_BORE_DEFINITION.noise_emission(), 24);
}

#[test]
fn power_roles_and_sockets_are_definition_owned() {
    assert_eq!(
        SOLAR_ARRAY_DEFINITION.power_role(),
        Some(PowerRole::Generator)
    );
    assert_eq!(PYLON_DEFINITION.power_role(), Some(PowerRole::Relay));
    assert_eq!(
        POWER_CONNECTOR_DEFINITION.power_role(),
        Some(PowerRole::Relay)
    );
    assert_eq!(POWER_CONNECTOR_DEFINITION.size(), [1, 1]);
    assert_eq!(POWER_CONNECTOR_DEFINITION.support(), FurnitureSupport::Side);
    assert_eq!(POWER_CONNECTOR_DEFINITION.power_connection_limit(), 5);
    assert_eq!(
        POWER_CONNECTOR_DEFINITION.power_connection_range_half_tiles(),
        16
    );
    assert_eq!(BATTERY_DEFINITION.power_role(), Some(PowerRole::Storage));
    assert_eq!(
        LASER_BORE_DEFINITION.power_role(),
        Some(PowerRole::Consumer)
    );
    assert_eq!(TURRET_DEFINITION.power_role(), Some(PowerRole::Consumer));
    assert_eq!(
        AMMO_TURRET_DEFINITION.power_role(),
        Some(PowerRole::Consumer)
    );
    assert_eq!(
        DIRECTIONAL_SENTRY_DEFINITION.power_role(),
        Some(PowerRole::Consumer)
    );
    assert_eq!(
        LASER_DRILL_DEFINITION.power_role(),
        Some(PowerRole::Consumer)
    );
    assert_eq!(TURRET_DEFINITION.power_socket_half_tiles(), Some([1, 0]));
    assert_eq!(
        ORBITAL_EXPORT_LAUNCHER_DEFINITION.power_role(),
        Some(PowerRole::Consumer)
    );
    assert_eq!(PYLON_DEFINITION.power_socket_half_tiles(), Some([0, 0]));
    assert_eq!(BATTERY_DEFINITION.power_socket_half_tiles(), Some([1, 0]));
    assert_eq!(
        BATTERY_DEFINITION.power_capacity_milli(),
        BATTERY_CAPACITY_MILLI
    );
    assert!(BATTERY_DEFINITION.interaction().is_interactive());
    assert!(BATTERY_DEFINITION.interaction().shows_power_storage());
}

#[test]
fn machinery_chunk_activity_is_definition_owned() {
    for definition in [
        LASER_BORE_DEFINITION,
        TURRET_DEFINITION,
        ORBITAL_EXPORT_LAUNCHER_DEFINITION,
        SOLAR_ARRAY_DEFINITION,
        BATTERY_DEFINITION,
        CARGO_LIFT_DEFINITION,
        LIFT_STATION_DEFINITION,
        COMPOSITE_ASSEMBLER_DEFINITION,
        RED_SHAFT_BORE_DEFINITION,
        LASER_DRILL_DEFINITION,
        AMMO_TURRET_DEFINITION,
        DIRECTIONAL_SENTRY_DEFINITION,
    ] {
        assert_eq!(definition.chunk_activity(), ChunkActivity::Nearby);
        assert_eq!(definition.maximum_health(), Some(DEFAULT_MACHINE_HEALTH));
        assert!(definition.lifeform_attention() > 0);
    }
    for definition in [
        CHEST_DEFINITION,
        CARGO_CONVEYOR_DEFINITION,
        PYLON_DEFINITION,
        POWERED_CABLE_ANCHOR_DEFINITION,
        POWER_CONNECTOR_DEFINITION,
    ] {
        assert_eq!(definition.chunk_activity(), ChunkActivity::Local);
        assert_eq!(definition.maximum_health(), None);
        assert_eq!(definition.lifeform_attention(), 0);
    }
}

#[test]
fn larger_and_more_power_hungry_machines_attract_more_attention() {
    assert!(
        RED_SHAFT_BORE_DEFINITION.lifeform_attention() > LASER_BORE_DEFINITION.lifeform_attention()
    );
    assert!(LASER_BORE_DEFINITION.lifeform_attention() > TURRET_DEFINITION.lifeform_attention());
}
