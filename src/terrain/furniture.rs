use super::ObjectTypeId;

/// Stable IDs for player-placeable furniture. Furniture occupies the same
/// indexed object layer as natural decorations, so tiles cannot be placed
/// through it and any footprint cell can resolve the complete object.
pub struct FurnitureObject;

impl FurnitureObject {
    pub const CHEST: ObjectTypeId = ObjectTypeId::new(256);
    pub const LASER_BORE: ObjectTypeId = ObjectTypeId::new(257);
    pub const TURRET: ObjectTypeId = ObjectTypeId::new(258);
    pub const ORBITAL_EXPORT_LAUNCHER: ObjectTypeId = ObjectTypeId::new(259);
    pub const CARGO_CONVEYOR: ObjectTypeId = ObjectTypeId::new(260);
    pub const SOLAR_ARRAY: ObjectTypeId = ObjectTypeId::new(261);
    pub const PYLON: ObjectTypeId = ObjectTypeId::new(262);
    pub const BATTERY: ObjectTypeId = ObjectTypeId::new(263);
    pub const POWERED_CABLE_ANCHOR: ObjectTypeId = ObjectTypeId::new(264);
    pub const CARGO_LIFT: ObjectTypeId = ObjectTypeId::new(265);
    pub const LIFT_STATION: ObjectTypeId = ObjectTypeId::new(266);
    pub const POWER_CONNECTOR: ObjectTypeId = ObjectTypeId::new(267);
}

pub const LASER_BORE_MAX_LENGTH: u32 = 400;
pub const LASER_BORE_TICKS_PER_TILE: u8 = 3;
pub const LASER_BORE_SLOTS: u16 = 10;
pub const ORBITAL_EXPORT_LAUNCHER_SLOTS: u16 = 8;
pub const POWER_CONNECTION_RANGE_TILES: f32 = 22.5;
pub const POWER_CONNECTION_RANGE_HALF_TILES: u16 = 45;
pub const POWER_CONNECTOR_RANGE_TILES: u16 = 8;
pub const SOLAR_GENERATION_MILLI_PER_SECOND: u32 = 12_000;
pub const LASER_BORE_DEMAND_MILLI_PER_SECOND: u32 = 8_000;
pub const ORBITAL_EXPORT_DEMAND_MILLI_PER_SECOND: u32 = 4_000;
pub const TURRET_DEMAND_MILLI_PER_SECOND: u32 = 6_000;
pub const BATTERY_CAPACITY_MILLI: u32 = 480_000;
pub const CARGO_LIFT_SLOTS: u16 = 20;
pub const CARGO_LIFT_DEMAND_MILLI_PER_SECOND: u32 = 10_000;
pub const CARGO_LIFT_SPEED_MILLI_TILES_PER_SECOND: u32 = 6_000;
pub const LIFT_STATION_SLOTS: u16 = 20;

/// Declares how furniture participates in the sparse power graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerRole {
    Generator,
    Relay,
    Consumer,
    Storage,
}

/// Declares how a generic container participates in cargo networks. Routing
/// behavior stays out of the transport system's furniture-type branches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemTransportRole {
    /// General storage may receive machine output or feed an input machine.
    Buffer,
    /// Output-only machinery, such as the laser bore.
    Output,
    /// Input-only machinery, such as the orbital export launcher.
    Input,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum TargetPriority {
    Weakest = 0,
    Strongest = 1,
    #[default]
    Closest = 2,
    Furthest = 3,
}

impl TargetPriority {
    pub const ALL: [Self; 4] = [
        Self::Weakest,
        Self::Strongest,
        Self::Closest,
        Self::Furthest,
    ];

    pub const fn raw(self) -> u8 {
        self as u8
    }

    pub const fn from_raw(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Weakest),
            1 => Some(Self::Strongest),
            2 => Some(Self::Closest),
            3 => Some(Self::Furthest),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Weakest => "WEAKEST",
            Self::Strongest => "STRONGEST",
            Self::Closest => "CLOSEST",
            Self::Furthest => "FURTHEST",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FurnitureConfiguration {
    TargetPriority,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum CargoLiftDirection {
    #[default]
    Idle = 0,
    Up = 1,
    Down = 2,
}

impl CargoLiftDirection {
    pub const fn from_raw(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Idle),
            1 => Some(Self::Up),
            2 => Some(Self::Down),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Up => "MOVING UP",
            Self::Down => "MOVING DOWN",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum LiftStationMode {
    #[default]
    Load = 0,
    Unload = 1,
}

impl LiftStationMode {
    pub const ALL: [Self; 2] = [Self::Load, Self::Unload];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Load => "LOAD",
            Self::Unload => "UNLOAD",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiftStationConfiguration {
    mode: LiftStationMode,
    departure: CargoLiftDirection,
}

impl LiftStationConfiguration {
    pub const DEFAULT: Self = Self {
        mode: LiftStationMode::Load,
        departure: CargoLiftDirection::Down,
    };

    pub const fn new(mode: LiftStationMode, departure: CargoLiftDirection) -> Option<Self> {
        match departure {
            CargoLiftDirection::Up | CargoLiftDirection::Down => Some(Self { mode, departure }),
            CargoLiftDirection::Idle => None,
        }
    }

    pub const fn from_raw(raw: u8) -> Option<Self> {
        let mode = match raw & 1 {
            0 => LiftStationMode::Load,
            1 => LiftStationMode::Unload,
            _ => unreachable!(),
        };
        let departure = match raw >> 1 {
            0 => CargoLiftDirection::Up,
            1 => CargoLiftDirection::Down,
            _ => return None,
        };
        Some(Self { mode, departure })
    }

    pub const fn raw(self) -> u8 {
        self.mode as u8 | ((self.departure as u8 - 1) << 1)
    }

    pub const fn mode(self) -> LiftStationMode {
        self.mode
    }

    pub const fn departure(self) -> CargoLiftDirection {
        self.departure
    }

    pub const fn with_mode(self, mode: LiftStationMode) -> Self {
        Self { mode, ..self }
    }

    pub const fn with_departure(self, departure: CargoLiftDirection) -> Option<Self> {
        Self::new(self.mode, departure)
    }
}

impl Default for LiftStationConfiguration {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LaserBoreBeam {
    pub(crate) x: u32,
    pub(crate) first_y: u32,
    pub(crate) length_tiles: u32,
    pub(crate) target: Option<super::TilePos>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FurnitureSupport {
    /// Every tile directly below the footprint must contain foreground terrain.
    Floor,
    /// Only the two outside cells below the footprint must contain terrain.
    /// This leaves the centre of machinery such as the laser bore unobstructed.
    FloorEdges,
    /// A foreground tile on any side or a background tile behind the object.
    Side,
    /// No terrain root is required. Placement rules owned by the furniture
    /// family provide any attachment constraints.
    Free,
}

impl FurnitureSupport {
    pub(crate) const fn requires_column(self, column: u16, width: u16) -> bool {
        match self {
            Self::Floor => true,
            Self::FloorEdges => column == 0 || column.saturating_add(1) == width,
            Self::Side => column == 0,
            Self::Free => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FurnitureInteraction {
    container_slots: Option<u16>,
    activatable: bool,
    configuration: Option<FurnitureConfiguration>,
    item_transport_role: Option<ItemTransportRole>,
    power_storage_status: bool,
    drill_depth_status: bool,
    kill_count_status: bool,
    lift_controls: bool,
    lift_station_controls: bool,
}

impl FurnitureInteraction {
    pub const NONE: Self = Self {
        container_slots: None,
        activatable: false,
        configuration: None,
        item_transport_role: None,
        power_storage_status: false,
        drill_depth_status: false,
        kill_count_status: false,
        lift_controls: false,
        lift_station_controls: false,
    };

    /// Creates a storage interaction without coupling the object store or UI to
    /// a particular furniture type.
    pub const fn container(slots: u16) -> Self {
        Self::container_with_transport(slots, ItemTransportRole::Buffer)
    }

    pub const fn container_with_transport(slots: u16, role: ItemTransportRole) -> Self {
        Self {
            container_slots: Some(slots),
            activatable: false,
            configuration: None,
            item_transport_role: Some(role),
            power_storage_status: false,
            drill_depth_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
        }
    }

    /// Creates an inventory-backed machine with a reusable activate/deactivate
    /// control. Newly placed machines always begin inactive.
    pub const fn machine(slots: u16) -> Self {
        Self::machine_with_transport(slots, ItemTransportRole::Buffer)
    }

    pub const fn machine_with_transport(slots: u16, role: ItemTransportRole) -> Self {
        Self {
            container_slots: Some(slots),
            activatable: true,
            configuration: None,
            item_transport_role: Some(role),
            power_storage_status: false,
            drill_depth_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
        }
    }

    /// Creates an activatable machine with a reusable configuration control
    /// but no inventory. Additional machine families can extend the
    /// configuration enum without coupling their simulation to the GUI.
    pub const fn controlled_machine(configuration: FurnitureConfiguration) -> Self {
        Self {
            container_slots: None,
            activatable: true,
            configuration: Some(configuration),
            item_transport_role: None,
            power_storage_status: false,
            drill_depth_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
        }
    }

    pub const fn power_storage() -> Self {
        Self {
            container_slots: None,
            activatable: false,
            configuration: None,
            item_transport_role: None,
            power_storage_status: true,
            drill_depth_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
        }
    }

    /// Creates inventory storage that is deliberately not an endpoint in the
    /// cargo-conveyor graph. Moving storage such as a lift owns its contents
    /// locally, so motion does not invalidate transport topology.
    pub const fn local_container(slots: u16) -> Self {
        Self {
            container_slots: Some(slots),
            activatable: false,
            configuration: None,
            item_transport_role: None,
            power_storage_status: false,
            drill_depth_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
        }
    }

    pub const fn with_drill_depth_status(mut self) -> Self {
        self.drill_depth_status = true;
        self
    }

    pub const fn with_kill_count_status(mut self) -> Self {
        self.kill_count_status = true;
        self
    }

    pub const fn with_lift_controls(mut self) -> Self {
        self.lift_controls = true;
        self
    }

    pub const fn with_lift_station_controls(mut self) -> Self {
        self.lift_station_controls = true;
        self
    }

    pub const fn container_slots(self) -> Option<u16> {
        self.container_slots
    }

    pub const fn is_activatable(self) -> bool {
        self.activatable
    }

    pub const fn configuration(self) -> Option<FurnitureConfiguration> {
        self.configuration
    }

    pub const fn item_transport_role(self) -> Option<ItemTransportRole> {
        self.item_transport_role
    }

    pub const fn shows_power_storage(self) -> bool {
        self.power_storage_status
    }

    pub const fn shows_drill_depth(self) -> bool {
        self.drill_depth_status
    }

    pub const fn shows_kill_count(self) -> bool {
        self.kill_count_status
    }

    pub const fn shows_lift_controls(self) -> bool {
        self.lift_controls
    }

    pub const fn shows_lift_station_controls(self) -> bool {
        self.lift_station_controls
    }

    pub const fn is_interactive(self) -> bool {
        self.container_slots.is_some()
            || self.activatable
            || self.configuration.is_some()
            || self.power_storage_status
            || self.drill_depth_status
            || self.kill_count_status
            || self.lift_controls
            || self.lift_station_controls
    }
}

/// Immutable placement and interaction metadata for one furniture type.
/// Adding another built-in furniture type only requires another definition;
/// the world object, occupancy, persistence, item-use, and rendering paths all
/// use these generic dimensions and support rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FurnitureDefinition {
    object_type: ObjectTypeId,
    name: &'static str,
    size: [u16; 2],
    support: FurnitureSupport,
    interaction: FurnitureInteraction,
    sprite_frame: u16,
    item_transport_connector: bool,
    power_role: Option<PowerRole>,
    /// Socket offset from the anchor in half-tile units. Integer storage keeps
    /// definitions const-friendly and exactly comparable.
    power_socket_half_tiles: [i16; 2],
    power_rate_milli_per_second: u32,
    power_capacity_milli: u32,
    power_connection_range_half_tiles: u16,
    power_connection_limit: u16,
}

impl FurnitureDefinition {
    pub const fn new(
        object_type: ObjectTypeId,
        name: &'static str,
        size: [u16; 2],
        support: FurnitureSupport,
        interaction: FurnitureInteraction,
        sprite_frame: u16,
    ) -> Self {
        Self {
            object_type,
            name,
            size,
            support,
            interaction,
            sprite_frame,
            item_transport_connector: false,
            power_role: None,
            power_socket_half_tiles: [0; 2],
            power_rate_milli_per_second: 0,
            power_capacity_milli: 0,
            power_connection_range_half_tiles: 0,
            power_connection_limit: 0,
        }
    }

    pub const fn with_item_transport_connector(mut self) -> Self {
        self.item_transport_connector = true;
        self
    }

    pub const fn with_power(mut self, role: PowerRole, socket_half_tiles: [i16; 2]) -> Self {
        self.power_role = Some(role);
        self.power_socket_half_tiles = socket_half_tiles;
        self
    }

    pub const fn with_relay_power(
        mut self,
        socket_half_tiles: [i16; 2],
        range_half_tiles: u16,
        connection_limit: u16,
    ) -> Self {
        self.power_role = Some(PowerRole::Relay);
        self.power_socket_half_tiles = socket_half_tiles;
        self.power_connection_range_half_tiles = range_half_tiles;
        self.power_connection_limit = connection_limit;
        self
    }

    pub const fn with_power_rate(mut self, milli_per_second: u32) -> Self {
        self.power_rate_milli_per_second = milli_per_second;
        self
    }

    pub const fn with_power_capacity(mut self, milli: u32) -> Self {
        self.power_capacity_milli = milli;
        self
    }

    pub const fn object_type(self) -> ObjectTypeId {
        self.object_type
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn size(self) -> [u16; 2] {
        self.size
    }

    pub const fn support(self) -> FurnitureSupport {
        self.support
    }

    pub const fn interaction(self) -> FurnitureInteraction {
        self.interaction
    }

    pub const fn sprite_frame(self) -> u16 {
        self.sprite_frame
    }

    pub const fn is_item_transport_connector(self) -> bool {
        self.item_transport_connector
    }

    pub const fn power_role(self) -> Option<PowerRole> {
        self.power_role
    }

    pub const fn power_socket_half_tiles(self) -> Option<[i16; 2]> {
        match self.power_role {
            Some(_) => Some(self.power_socket_half_tiles),
            None => None,
        }
    }

    pub const fn power_rate_milli_per_second(self) -> u32 {
        self.power_rate_milli_per_second
    }

    pub const fn power_capacity_milli(self) -> u32 {
        self.power_capacity_milli
    }

    pub const fn power_connection_range_half_tiles(self) -> u16 {
        self.power_connection_range_half_tiles
    }

    pub const fn power_connection_limit(self) -> u16 {
        self.power_connection_limit
    }
}

pub const CHEST_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::CHEST,
    "WOODEN CHEST",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::container(40),
    0,
);

pub const LASER_BORE_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::LASER_BORE,
    "LASER BORE",
    [3, 3],
    FurnitureSupport::FloorEdges,
    FurnitureInteraction::machine_with_transport(LASER_BORE_SLOTS, ItemTransportRole::Output)
        .with_drill_depth_status(),
    1,
)
.with_power(PowerRole::Consumer, [2, 0])
.with_power_rate(LASER_BORE_DEMAND_MILLI_PER_SECOND);

pub const TURRET_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::TURRET,
    "DEFENCE TURRET",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::controlled_machine(FurnitureConfiguration::TargetPriority)
        .with_kill_count_status(),
    2,
)
.with_power(PowerRole::Consumer, [1, 0])
.with_power_rate(TURRET_DEMAND_MILLI_PER_SECOND);

pub const ORBITAL_EXPORT_LAUNCHER_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
    "ORBITAL EXPORT LAUNCHER",
    [3, 3],
    FurnitureSupport::Floor,
    FurnitureInteraction::container_with_transport(
        ORBITAL_EXPORT_LAUNCHER_SLOTS,
        ItemTransportRole::Input,
    ),
    3,
)
.with_power(PowerRole::Consumer, [2, 0])
.with_power_rate(ORBITAL_EXPORT_DEMAND_MILLI_PER_SECOND);

pub const CARGO_CONVEYOR_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::CARGO_CONVEYOR,
    "CARGO CONVEYOR",
    [1, 1],
    FurnitureSupport::Free,
    FurnitureInteraction::NONE,
    4,
)
.with_item_transport_connector();

pub const SOLAR_ARRAY_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::SOLAR_ARRAY,
    "SOLAR ARRAY",
    [2, 3],
    FurnitureSupport::Floor,
    FurnitureInteraction::NONE,
    10,
)
.with_power(PowerRole::Generator, [1, 1])
.with_power_rate(SOLAR_GENERATION_MILLI_PER_SECOND);

pub const PYLON_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::PYLON,
    "PYLON",
    [1, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::NONE,
    11,
)
.with_relay_power([0, 0], POWER_CONNECTION_RANGE_HALF_TILES, 10);

pub const BATTERY_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::BATTERY,
    "BATTERY",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::power_storage(),
    12,
)
.with_power(PowerRole::Storage, [1, 0])
.with_power_capacity(BATTERY_CAPACITY_MILLI);

pub const POWERED_CABLE_ANCHOR_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::POWERED_CABLE_ANCHOR,
    "POWERED CABLE ANCHOR",
    [1, 1],
    FurnitureSupport::Free,
    FurnitureInteraction::NONE,
    13,
)
.with_relay_power([0, 0], POWER_CONNECTION_RANGE_HALF_TILES, 2);

pub const POWER_CONNECTOR_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::POWER_CONNECTOR,
    "POWER CONNECTOR",
    [1, 1],
    FurnitureSupport::Side,
    FurnitureInteraction::NONE,
    11,
)
.with_relay_power([0, 0], POWER_CONNECTOR_RANGE_TILES * 2, 5);

pub const CARGO_LIFT_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::CARGO_LIFT,
    "CARGO LIFT",
    [2, 2],
    FurnitureSupport::Free,
    FurnitureInteraction::local_container(CARGO_LIFT_SLOTS).with_lift_controls(),
    14,
)
.with_power(PowerRole::Consumer, [0, 0])
.with_power_rate(CARGO_LIFT_DEMAND_MILLI_PER_SECOND);

pub const LIFT_STATION_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::LIFT_STATION,
    "LIFT STATION",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::container(LIFT_STATION_SLOTS).with_lift_station_controls(),
    15,
);

/// The single registration table for built-in furniture metadata. Placement,
/// persistence, interaction UI, and rendering all resolve definitions here.
pub const BUILT_IN_FURNITURE: &[FurnitureDefinition] = &[
    CHEST_DEFINITION,
    LASER_BORE_DEFINITION,
    TURRET_DEFINITION,
    ORBITAL_EXPORT_LAUNCHER_DEFINITION,
    CARGO_CONVEYOR_DEFINITION,
    SOLAR_ARRAY_DEFINITION,
    PYLON_DEFINITION,
    BATTERY_DEFINITION,
    POWERED_CABLE_ANCHOR_DEFINITION,
    CARGO_LIFT_DEFINITION,
    LIFT_STATION_DEFINITION,
    POWER_CONNECTOR_DEFINITION,
];

pub fn furniture_definition(object_type: ObjectTypeId) -> Option<FurnitureDefinition> {
    BUILT_IN_FURNITURE
        .iter()
        .copied()
        .find(|definition| definition.object_type == object_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_furniture_ids_are_unique() {
        for (index, definition) in BUILT_IN_FURNITURE.iter().enumerate() {
            assert!(
                BUILT_IN_FURNITURE[index + 1..]
                    .iter()
                    .all(|other| other.object_type != definition.object_type)
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
}
