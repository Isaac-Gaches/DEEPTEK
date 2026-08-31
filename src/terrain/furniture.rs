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
    pub const COMPOSITE_ASSEMBLER: ObjectTypeId = ObjectTypeId::new(268);
    pub const RED_SHAFT_BORE: ObjectTypeId = ObjectTypeId::new(269);
    pub const PROCUREMENT_TERMINAL: ObjectTypeId = ObjectTypeId::new(270);
    pub const LASER_DRILL: ObjectTypeId = ObjectTypeId::new(271);
    pub const AMMO_TURRET: ObjectTypeId = ObjectTypeId::new(272);
    pub const DIRECTIONAL_SENTRY: ObjectTypeId = ObjectTypeId::new(273);
    pub const SPIKES: ObjectTypeId = ObjectTypeId::new(274);
    pub const DOOR: ObjectTypeId = ObjectTypeId::new(275);
    pub const BED: ObjectTypeId = ObjectTypeId::new(276);
    pub const SUBSURFACE_SURVEYOR: ObjectTypeId = ObjectTypeId::new(277);
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
pub const AMMO_TURRET_DEMAND_MILLI_PER_SECOND: u32 = 4_000;
pub const AMMO_TURRET_SLOTS: u16 = 4;
pub const DIRECTIONAL_SENTRY_DEMAND_MILLI_PER_SECOND: u32 = 2_500;
pub const BATTERY_CAPACITY_MILLI: u32 = 480_000;
pub const CARGO_LIFT_SLOTS: u16 = 20;
pub const CARGO_LIFT_DEMAND_MILLI_PER_SECOND: u32 = 10_000;
pub const CARGO_LIFT_SPEED_MILLI_TILES_PER_SECOND: u32 = 6_000;
pub const LIFT_STATION_SLOTS: u16 = 20;
pub const COMPOSITE_ASSEMBLER_SLOTS: u16 = 3;
pub const COMPOSITE_ASSEMBLER_DEMAND_MILLI_PER_SECOND: u32 = 7_000;
pub const RED_SHAFT_BORE_WIDTH: u32 = 4;
pub const RED_SHAFT_BORE_SLOTS: u16 = 40;
pub const RED_SHAFT_BORE_DEMAND_MILLI_PER_SECOND: u32 = 24_000;
pub const LASER_DRILL_MAX_LENGTH: u32 = 160;
pub const LASER_DRILL_SLOTS: u16 = 10;
pub const LASER_DRILL_DEMAND_MILLI_PER_SECOND: u32 = 12_000;
pub const SUBSURFACE_SURVEY_WIDTH: u32 = 64;
pub const SUBSURFACE_SURVEY_DEPTH: u32 = 1_000;
pub const SUBSURFACE_SURVEYOR_DEMAND_MILLI_PER_SECOND: u32 = 3_000;
pub const PROCUREMENT_TERMINAL_DEMAND_MILLI_PER_SECOND: u32 = 1_000;

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
    /// Two-input machines expose recipe inputs and their output separately.
    Processor,
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
    LaserAim,
}

const FACING_LEFT_FLAG: u8 = 1 << 7;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FurnitureFacing {
    Left,
    #[default]
    Right,
}

impl FurnitureFacing {
    pub const fn horizontal_sign(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }

    pub(crate) const fn from_variant(variant: u8) -> Self {
        if variant & FACING_LEFT_FLAG == 0 {
            Self::Right
        } else {
            Self::Left
        }
    }

    pub(crate) const fn apply_to_variant(self, variant: u8) -> u8 {
        match self {
            Self::Left => variant | FACING_LEFT_FLAG,
            Self::Right => variant & !FACING_LEFT_FLAG,
        }
    }
}

pub(crate) const fn configuration_variant(variant: u8) -> u8 {
    variant & !FACING_LEFT_FLAG
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum LaserDrillAim {
    FarLeft = 0,
    Left = 1,
    SlightLeft = 2,
    #[default]
    Down = 3,
    SlightRight = 4,
    Right = 5,
    FarRight = 6,
}

impl LaserDrillAim {
    pub const ALL: [Self; 7] = [
        Self::FarLeft,
        Self::Left,
        Self::SlightLeft,
        Self::Down,
        Self::SlightRight,
        Self::Right,
        Self::FarRight,
    ];

    pub const fn raw(self) -> u8 {
        self as u8
    }

    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::FarLeft),
            1 => Some(Self::Left),
            2 => Some(Self::SlightLeft),
            3 => Some(Self::Down),
            4 => Some(Self::SlightRight),
            5 => Some(Self::Right),
            6 => Some(Self::FarRight),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::FarLeft => "63L",
            Self::Left => "45L",
            Self::SlightLeft => "27L",
            Self::Down => "DOWN",
            Self::SlightRight => "27R",
            Self::Right => "45R",
            Self::FarRight => "63R",
        }
    }

    pub const fn direction(self) -> [i32; 2] {
        match self {
            Self::FarLeft => [-2, 1],
            Self::Left => [-1, 1],
            Self::SlightLeft => [-1, 2],
            Self::Down => [0, 1],
            Self::SlightRight => [1, 2],
            Self::Right => [1, 1],
            Self::FarRight => [2, 1],
        }
    }

    pub(crate) fn tile_offset(self, step: u32) -> [i32; 2] {
        let [x, y] = self.direction();
        let divisor = x.unsigned_abs().max(y.unsigned_abs()).max(1) as i64;
        let step = i64::from(step);
        [
            rounded_ratio(i64::from(x) * step, divisor),
            rounded_ratio(i64::from(y) * step, divisor),
        ]
    }
}

fn rounded_ratio(value: i64, divisor: i64) -> i32 {
    let magnitude = (value.unsigned_abs() + divisor as u64 / 2) / divisor as u64;
    if value < 0 {
        -(magnitude as i32)
    } else {
        magnitude as i32
    }
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
pub(crate) struct RedShaftBoreBeam {
    pub(crate) first_x: u32,
    pub(crate) width: u32,
    pub(crate) first_y: u32,
    pub(crate) length_tiles: u32,
    pub(crate) target_y: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LaserDrillBeam {
    pub(crate) origin: [f32; 2],
    pub(crate) endpoint: [f32; 2],
    pub(crate) first_tile: super::TilePos,
    pub(crate) steps: u32,
    pub(crate) aim: LaserDrillAim,
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
    subsurface_survey_status: bool,
    kill_count_status: bool,
    lift_controls: bool,
    lift_station_controls: bool,
    procurement_terminal: bool,
    door: bool,
    bed: bool,
}

impl FurnitureInteraction {
    pub const NONE: Self = Self {
        container_slots: None,
        activatable: false,
        configuration: None,
        item_transport_role: None,
        power_storage_status: false,
        drill_depth_status: false,
        subsurface_survey_status: false,
        kill_count_status: false,
        lift_controls: false,
        lift_station_controls: false,
        procurement_terminal: false,
        door: false,
        bed: false,
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
            subsurface_survey_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
            procurement_terminal: false,
            door: false,
            bed: false,
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
            subsurface_survey_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
            procurement_terminal: false,
            door: false,
            bed: false,
        }
    }

    pub const fn with_configuration(mut self, configuration: FurnitureConfiguration) -> Self {
        self.configuration = Some(configuration);
        self
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
            subsurface_survey_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
            procurement_terminal: false,
            door: false,
            bed: false,
        }
    }

    pub const fn activatable_machine() -> Self {
        Self {
            activatable: true,
            ..Self::NONE
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
            subsurface_survey_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
            procurement_terminal: false,
            door: false,
            bed: false,
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
            subsurface_survey_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
            procurement_terminal: false,
            door: false,
            bed: false,
        }
    }

    pub const fn procurement_terminal() -> Self {
        Self {
            container_slots: None,
            activatable: false,
            configuration: None,
            item_transport_role: None,
            power_storage_status: false,
            drill_depth_status: false,
            subsurface_survey_status: false,
            kill_count_status: false,
            lift_controls: false,
            lift_station_controls: false,
            procurement_terminal: true,
            door: false,
            bed: false,
        }
    }

    pub const fn door() -> Self {
        Self {
            door: true,
            ..Self::NONE
        }
    }

    pub const fn bed() -> Self {
        Self {
            bed: true,
            ..Self::NONE
        }
    }

    pub const fn with_drill_depth_status(mut self) -> Self {
        self.drill_depth_status = true;
        self
    }

    pub const fn with_subsurface_survey_status(mut self) -> Self {
        self.subsurface_survey_status = true;
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

    pub const fn shows_subsurface_survey(self) -> bool {
        self.subsurface_survey_status
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

    pub const fn opens_procurement(self) -> bool {
        self.procurement_terminal
    }

    pub const fn toggles_door(self) -> bool {
        self.door
    }

    pub const fn allows_sleep(self) -> bool {
        self.bed
    }

    pub const fn is_interactive(self) -> bool {
        self.container_slots.is_some()
            || self.activatable
            || self.configuration.is_some()
            || self.power_storage_status
            || self.drill_depth_status
            || self.subsurface_survey_status
            || self.kill_count_status
            || self.lift_controls
            || self.lift_station_controls
            || self.procurement_terminal
            || self.door
            || self.bed
    }
}

/// Immutable placement and interaction metadata for one furniture type.
/// Adding another built-in furniture type only requires another definition;
/// the world object, occupancy, persistence, item-use, and rendering paths all
/// use these generic dimensions and support rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkActivity {
    None,
    Local,
    Nearby,
}

pub const DEFAULT_MACHINE_HEALTH: u16 = 200;
const LIFEFORM_ATTENTION_POWER_STEP_MILLI_PER_SECOND: u32 = 4_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FurnitureDefinition {
    object_type: ObjectTypeId,
    name: &'static str,
    size: [u16; 2],
    support: FurnitureSupport,
    interaction: FurnitureInteraction,
    sprite_frame: u16,
    item_transport_connector: bool,
    chunk_activity: ChunkActivity,
    maximum_health: u16,
    lifeform_attention: u32,
    power_role: Option<PowerRole>,
    /// Socket offset from the anchor in half-tile units. Integer storage keeps
    /// definitions const-friendly and exactly comparable.
    power_socket_half_tiles: [i16; 2],
    power_rate_milli_per_second: u32,
    power_capacity_milli: u32,
    power_connection_range_half_tiles: u16,
    power_connection_limit: u16,
    supports_facing: bool,
    structural: bool,
    room_boundary: bool,
    noise_emission: u16,
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
            chunk_activity: ChunkActivity::None,
            maximum_health: 0,
            lifeform_attention: 0,
            power_role: None,
            power_socket_half_tiles: [0; 2],
            power_rate_milli_per_second: 0,
            power_capacity_milli: 0,
            power_connection_range_half_tiles: 0,
            power_connection_limit: 0,
            supports_facing: false,
            structural: false,
            room_boundary: false,
            noise_emission: 0,
        }
    }

    pub const fn with_item_transport_connector(mut self) -> Self {
        self.item_transport_connector = true;
        self
    }

    /// Marks working machinery as a simulation anchor for nearby chunks.
    pub const fn with_chunk_activity(mut self) -> Self {
        self.chunk_activity = ChunkActivity::Nearby;
        self.maximum_health = DEFAULT_MACHINE_HEALTH;
        // Footprint is the baseline industrial signature, so physically larger
        // machines draw more attention without requiring spawner-side type checks.
        self.lifeform_attention = self.size[0] as u32 * self.size[1] as u32;
        self
    }

    /// Keeps only the chunks occupied by passive infrastructure simulated.
    pub const fn with_local_chunk_activity(mut self) -> Self {
        self.chunk_activity = ChunkActivity::Local;
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
        if matches!(self.chunk_activity, ChunkActivity::Nearby) && milli_per_second > 0 {
            self.lifeform_attention = self.lifeform_attention.saturating_add(
                milli_per_second.saturating_add(LIFEFORM_ATTENTION_POWER_STEP_MILLI_PER_SECOND - 1)
                    / LIFEFORM_ATTENTION_POWER_STEP_MILLI_PER_SECOND,
            );
        }
        self
    }

    pub const fn with_power_capacity(mut self, milli: u32) -> Self {
        self.power_capacity_milli = milli;
        self
    }

    pub const fn with_facing(mut self) -> Self {
        self.supports_facing = true;
        self
    }

    /// Makes every occupied cell solid and eligible as placement support.
    /// Collision and placement resolve this through the world's object cell index.
    pub const fn with_structural_collision(mut self) -> Self {
        self.structural = true;
        self
    }

    /// Marks furniture cells as closed boundaries for bounded room discovery.
    /// This is separate from collision so an animated/openable door can later
    /// change traversal without changing the housing representation.
    pub const fn with_room_boundary(mut self) -> Self {
        self.room_boundary = true;
        self
    }

    /// Declares definition-driven industrial noise for specialist happiness.
    pub const fn with_noise_emission(mut self, noise: u16) -> Self {
        self.noise_emission = noise;
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

    pub const fn chunk_activity(self) -> ChunkActivity {
        self.chunk_activity
    }

    pub const fn maximum_health(self) -> Option<u16> {
        if self.maximum_health == 0 {
            None
        } else {
            Some(self.maximum_health)
        }
    }

    /// Relative amount of hostile attention generated while this machine is active.
    /// Passive infrastructure deliberately returns zero.
    pub const fn lifeform_attention(self) -> u32 {
        self.lifeform_attention
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

    pub const fn supports_facing(self) -> bool {
        self.supports_facing
    }

    pub const fn is_structural(self) -> bool {
        self.structural
    }

    pub const fn is_room_boundary(self) -> bool {
        self.room_boundary
    }

    pub const fn noise_emission(self) -> u16 {
        self.noise_emission
    }
}

pub const CHEST_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::CHEST,
    "WOODEN CHEST",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::container(40),
    0,
)
.with_local_chunk_activity();

pub const LASER_BORE_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::LASER_BORE,
    "LASER BORE",
    [3, 3],
    FurnitureSupport::FloorEdges,
    FurnitureInteraction::machine_with_transport(LASER_BORE_SLOTS, ItemTransportRole::Output)
        .with_drill_depth_status(),
    1,
)
.with_chunk_activity()
.with_noise_emission(8)
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
.with_chunk_activity()
.with_facing()
.with_power(PowerRole::Consumer, [1, 0])
.with_power_rate(TURRET_DEMAND_MILLI_PER_SECOND);

pub const AMMO_TURRET_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::AMMO_TURRET,
    "AUTOCANNON TURRET",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::machine_with_transport(AMMO_TURRET_SLOTS, ItemTransportRole::Input)
        .with_configuration(FurnitureConfiguration::TargetPriority)
        .with_kill_count_status(),
    0,
)
.with_chunk_activity()
.with_facing()
.with_power(PowerRole::Consumer, [1, 0])
.with_power_rate(AMMO_TURRET_DEMAND_MILLI_PER_SECOND);

pub const DIRECTIONAL_SENTRY_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::DIRECTIONAL_SENTRY,
    "DIRECTIONAL SENTRY",
    [1, 1],
    FurnitureSupport::Side,
    FurnitureInteraction::activatable_machine().with_kill_count_status(),
    0,
)
.with_chunk_activity()
.with_facing()
.with_structural_collision()
.with_power(PowerRole::Consumer, [0, 0])
.with_power_rate(DIRECTIONAL_SENTRY_DEMAND_MILLI_PER_SECOND);

pub const SPIKES_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::SPIKES,
    "SPIKES",
    [1, 1],
    FurnitureSupport::Floor,
    FurnitureInteraction::NONE,
    0,
);

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
.with_chunk_activity()
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
.with_item_transport_connector()
.with_local_chunk_activity();

pub const SOLAR_ARRAY_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::SOLAR_ARRAY,
    "SOLAR ARRAY",
    [2, 3],
    FurnitureSupport::Floor,
    FurnitureInteraction::NONE,
    10,
)
.with_chunk_activity()
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
.with_local_chunk_activity()
.with_relay_power([0, 0], POWER_CONNECTION_RANGE_HALF_TILES, 10);

pub const BATTERY_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::BATTERY,
    "BATTERY",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::power_storage(),
    12,
)
.with_chunk_activity()
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
.with_local_chunk_activity();

pub const POWER_CONNECTOR_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::POWER_CONNECTOR,
    "POWER CONNECTOR",
    [1, 1],
    FurnitureSupport::Side,
    FurnitureInteraction::NONE,
    11,
)
.with_local_chunk_activity()
.with_relay_power([0, 0], POWER_CONNECTOR_RANGE_TILES * 2, 5);

pub const CARGO_LIFT_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::CARGO_LIFT,
    "CARGO LIFT",
    [2, 2],
    FurnitureSupport::Free,
    FurnitureInteraction::local_container(CARGO_LIFT_SLOTS).with_lift_controls(),
    14,
)
.with_chunk_activity()
.with_power(PowerRole::Consumer, [0, 0])
.with_power_rate(CARGO_LIFT_DEMAND_MILLI_PER_SECOND);

pub const LIFT_STATION_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::LIFT_STATION,
    "LIFT STATION",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::container(LIFT_STATION_SLOTS).with_lift_station_controls(),
    15,
)
.with_chunk_activity();

pub const COMPOSITE_ASSEMBLER_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::COMPOSITE_ASSEMBLER,
    "RESOURCE PROCESSOR",
    [3, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::machine_with_transport(
        COMPOSITE_ASSEMBLER_SLOTS,
        ItemTransportRole::Processor,
    ),
    0,
)
.with_chunk_activity()
.with_power(PowerRole::Consumer, [2, 0])
.with_power_rate(COMPOSITE_ASSEMBLER_DEMAND_MILLI_PER_SECOND);

pub const RED_SHAFT_BORE_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::RED_SHAFT_BORE,
    "RED SHAFT BORE",
    [6, 3],
    FurnitureSupport::FloorEdges,
    FurnitureInteraction::machine_with_transport(RED_SHAFT_BORE_SLOTS, ItemTransportRole::Output)
        .with_drill_depth_status(),
    0,
)
.with_chunk_activity()
.with_noise_emission(24)
.with_power(PowerRole::Consumer, [5, 0])
.with_power_rate(RED_SHAFT_BORE_DEMAND_MILLI_PER_SECOND);

pub const PROCUREMENT_TERMINAL_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::PROCUREMENT_TERMINAL,
    "PROCUREMENT TERMINAL",
    [2, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::procurement_terminal(),
    0,
)
.with_local_chunk_activity()
.with_power(PowerRole::Consumer, [1, 0])
.with_power_rate(PROCUREMENT_TERMINAL_DEMAND_MILLI_PER_SECOND);

pub const LASER_DRILL_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::LASER_DRILL,
    "LASER DRILL",
    [3, 2],
    FurnitureSupport::FloorEdges,
    FurnitureInteraction::machine_with_transport(LASER_DRILL_SLOTS, ItemTransportRole::Output)
        .with_configuration(FurnitureConfiguration::LaserAim)
        .with_drill_depth_status(),
    0,
)
.with_chunk_activity()
.with_noise_emission(12)
.with_power(PowerRole::Consumer, [2, 0])
.with_power_rate(LASER_DRILL_DEMAND_MILLI_PER_SECOND);

pub const DOOR_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::DOOR,
    "SETTLEMENT DOOR",
    [1, 3],
    FurnitureSupport::Floor,
    FurnitureInteraction::door(),
    0,
)
.with_room_boundary();

pub const BED_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::BED,
    "SETTLEMENT BED",
    [2, 1],
    FurnitureSupport::Floor,
    FurnitureInteraction::bed(),
    0,
);

pub const SUBSURFACE_SURVEYOR_DEFINITION: FurnitureDefinition = FurnitureDefinition::new(
    FurnitureObject::SUBSURFACE_SURVEYOR,
    "SUBSURFACE SURVEYOR",
    [3, 2],
    FurnitureSupport::Floor,
    FurnitureInteraction::activatable_machine().with_subsurface_survey_status(),
    0,
)
.with_chunk_activity()
.with_power(PowerRole::Consumer, [2, 0])
.with_power_rate(SUBSURFACE_SURVEYOR_DEMAND_MILLI_PER_SECOND);

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
    COMPOSITE_ASSEMBLER_DEFINITION,
    RED_SHAFT_BORE_DEFINITION,
    PROCUREMENT_TERMINAL_DEFINITION,
    LASER_DRILL_DEFINITION,
    AMMO_TURRET_DEFINITION,
    DIRECTIONAL_SENTRY_DEFINITION,
    SPIKES_DEFINITION,
    DOOR_DEFINITION,
    BED_DEFINITION,
    SUBSURFACE_SURVEYOR_DEFINITION,
];

pub fn furniture_definition(object_type: ObjectTypeId) -> Option<FurnitureDefinition> {
    const FIRST_FURNITURE_ID: u16 = 256;
    let index = object_type.raw().checked_sub(FIRST_FURNITURE_ID)? as usize;
    BUILT_IN_FURNITURE
        .get(index)
        .copied()
        .filter(|definition| definition.object_type == object_type)
}

#[cfg(test)]
mod tests;
