use crate::{
    BUILT_IN_FURNITURE, Contract, ContractBoard, ContractId, DeliverySystem, FurnitureObject,
    Inventory, ItemId, PROGRAM_DELIVERY_DELAY_SECONDS, PowerSystem, SubsurfaceSurvey, Transmission,
    TransmissionLog, World, assess_room,
};

pub const TUTORIAL_BRIEFING_DELAY_SECONDS: f32 = 5.0;
pub const MISSION_TRANSMISSION_DELAY_SECONDS: f32 = 4.0;
const MISSION_TRANSMISSION_DELAY_MILLISECONDS: u32 = 4_000;
const PACKAGE_MISSION_COUNT: usize = 5;

const BRIEFING: &str = "DEEPTEK Prospector #137, your extraction licence is active. Your mission is to extract and export the Asterite deposits estimated 10 km below the surface. Progress through the DEEPTEK Prospector Program in the Contracts menu.";
const INITIAL_IDS: [ContractId; 11] = [
    ContractId::BreakingGround,
    ContractId::SitePower,
    ContractId::FirstShipment,
    ContractId::Procurement,
    ContractId::IndustrialExtraction,
    ContractId::Prospecting,
    ContractId::IronAge,
    ContractId::GoingDown,
    ContractId::ValueAdded,
    ContractId::HandsOff,
    ContractId::HelpWanted,
];
const DEPTH_IDS: [ContractId; 6] = [
    ContractId::Depth100,
    ContractId::Depth250,
    ContractId::Depth500,
    ContractId::Depth1000,
    ContractId::Depth2500,
    ContractId::Depth5000,
];
const EXPANSION_IDS: [ContractId; 4] = [
    ContractId::IndustrialExtraction,
    ContractId::Prospecting,
    ContractId::IronAge,
    ContractId::GoingDown,
];
const CONVEYOR_PACKAGE: [ItemId; 20] = [ItemId::CARGO_CONVEYOR; 20];

const SOLAR_PLACED: u64 = 1 << 0;
const EXPORTER_POWERED: u64 = 1 << 1;
const TERMINAL_POWERED: u64 = 1 << 2;
const PURCHASED_ITEM: u64 = 1 << 3;
const DRILL_PLACED: u64 = 1 << 4;
const DRILL_POWERED: u64 = 1 << 5;
const DRILL_EXTRACTED: u64 = 1 << 6;
const SCANNER_FOUND_IRON: u64 = 1 << 7;
const PROCESSOR_PLACED: u64 = 1 << 8;
const PROCESSOR_POWERED: u64 = 1 << 9;
const AUTOMATIC_TRANSFER: u64 = 1 << 10;
const AUTOMATIC_PROCESSING: u64 = 1 << 11;
const SUITABLE_ACCOMMODATION: u64 = 1 << 12;
const SPECIALIST_RECRUITED: u64 = 1 << 13;
const SPECIALIST_MOVED_IN: u64 = 1 << 14;
const SCANNER_FOUND_ASTERITE: u64 = 1 << 15;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SavedProspectorProgress {
    pub completed_missions: u64,
    pub issued_packages: u64,
    pub facts: u64,
    pub stone_extracted: u64,
    pub stone_exported: u64,
    pub iron_acquired: u64,
    pub iron_processed: u64,
    pub asterite_acquired: u64,
    pub asterite_exported: u64,
    pub maximum_depth_decimetres: u32,
    pub package_transmission_delays_ms: [u32; PACKAGE_MISSION_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TutorialStage {
    Disabled,
    BriefingPending(f32),
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TutorialProgram {
    stage: TutorialStage,
    progress: SavedProspectorProgress,
}

impl Eq for TutorialProgram {}

impl Default for TutorialProgram {
    fn default() -> Self {
        Self {
            stage: TutorialStage::Disabled,
            progress: SavedProspectorProgress::default(),
        }
    }
}

impl TutorialProgram {
    pub const fn for_new_world() -> Self {
        Self {
            stage: TutorialStage::BriefingPending(TUTORIAL_BRIEFING_DELAY_SECONDS),
            progress: SavedProspectorProgress {
                completed_missions: 0,
                issued_packages: 0,
                facts: 0,
                stone_extracted: 0,
                stone_exported: 0,
                iron_acquired: 0,
                iron_processed: 0,
                asterite_acquired: 0,
                asterite_exported: 0,
                maximum_depth_decimetres: 0,
                package_transmission_delays_ms: [0; PACKAGE_MISSION_COUNT],
            },
        }
    }

    pub fn record_extraction(&mut self, item: ItemId, quantity: u64) {
        match item {
            ItemId::STONE_BLOCK => {
                self.progress.stone_extracted =
                    self.progress.stone_extracted.saturating_add(quantity);
            }
            ItemId::IRON_ORE => {
                self.progress.iron_acquired = self.progress.iron_acquired.saturating_add(quantity);
            }
            ItemId::ASTERITE => {
                self.progress.asterite_acquired =
                    self.progress.asterite_acquired.saturating_add(quantity);
            }
            _ => {}
        }
    }

    pub fn record_drill_extraction(&mut self, item: ItemId, quantity: u64) {
        self.progress.facts |= DRILL_EXTRACTED;
        self.record_extraction(item, quantity);
    }

    pub fn record_export(&mut self, item: ItemId, quantity: u64) {
        match item {
            ItemId::STONE_BLOCK => {
                self.progress.stone_exported =
                    self.progress.stone_exported.saturating_add(quantity);
            }
            ItemId::ASTERITE => {
                self.progress.asterite_exported =
                    self.progress.asterite_exported.saturating_add(quantity);
            }
            _ => {}
        }
    }

    pub fn record_purchase(&mut self) {
        self.progress.facts |= PURCHASED_ITEM;
    }

    pub fn record_iron_processing(&mut self, quantity: u64) {
        self.progress.iron_processed = self.progress.iron_processed.saturating_add(quantity);
        if self.progress.facts & AUTOMATIC_TRANSFER != 0 && quantity > 0 {
            self.progress.facts |= AUTOMATIC_PROCESSING;
        }
    }

    pub fn record_automatic_processor_transfer(&mut self, quantity: u64) {
        if quantity > 0 {
            self.progress.facts |= AUTOMATIC_TRANSFER;
        }
    }

    pub fn record_scanner_result(&mut self, survey: SubsurfaceSurvey) {
        for estimate in survey.estimates() {
            if estimate.estimated_yield == 0 {
                continue;
            }
            match estimate.item {
                ItemId::IRON_ORE => self.progress.facts |= SCANNER_FOUND_IRON,
                ItemId::ASTERITE => self.progress.facts |= SCANNER_FOUND_ASTERITE,
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        elapsed_seconds: f32,
        contracts: &mut ContractBoard,
        transmissions: &mut TransmissionLog,
        deliveries: &mut DeliverySystem,
        world: &World,
        power: &PowerSystem,
        inventory: &Inventory,
        depth_decimetres: i32,
    ) {
        self.observe_world(world, power, inventory, depth_decimetres);
        let elapsed = if elapsed_seconds.is_finite() {
            elapsed_seconds.max(0.0)
        } else {
            0.0
        };
        match self.stage {
            TutorialStage::Disabled => return,
            TutorialStage::BriefingPending(remaining) if elapsed + f32::EPSILON >= remaining => {
                transmissions.receive(Transmission::new(
                    "DEEPTEK OPERATIONS",
                    "EXTRACTION LICENCE ACTIVE",
                    BRIEFING,
                ));
                self.stage = TutorialStage::Active;
            }
            TutorialStage::BriefingPending(remaining) => {
                self.stage = TutorialStage::BriefingPending(remaining - elapsed);
                return;
            }
            TutorialStage::Active => {}
        }

        self.observe_legacy_contract_progress(contracts);

        self.advance_package_transmission_delays(elapsed, transmissions, deliveries);

        for _ in 0..4 {
            self.unlock_missions(contracts);
            self.sync_progress(contracts);
            self.capture_completions(contracts);
        }
    }

    fn observe_world(
        &mut self,
        world: &World,
        power: &PowerSystem,
        inventory: &Inventory,
        depth_decimetres: i32,
    ) {
        let has_type = |object_type| world.objects_of_type(object_type).next().is_some();
        let powered_type = |object_type| {
            world
                .objects_of_type(object_type)
                .any(|object| power.is_powered(object.id()))
        };
        if has_type(FurnitureObject::SOLAR_ARRAY) {
            self.progress.facts |= SOLAR_PLACED;
        }
        if powered_type(FurnitureObject::ORBITAL_EXPORT_LAUNCHER) {
            self.progress.facts |= EXPORTER_POWERED;
        }
        if powered_type(FurnitureObject::PROCUREMENT_TERMINAL) {
            self.progress.facts |= TERMINAL_POWERED;
        }
        let drill_types = [
            FurnitureObject::LASER_BORE,
            FurnitureObject::RED_SHAFT_BORE,
            FurnitureObject::LASER_DRILL,
        ];
        if drill_types.into_iter().any(has_type) {
            self.progress.facts |= DRILL_PLACED;
        }
        if drill_types.into_iter().any(powered_type) {
            self.progress.facts |= DRILL_POWERED;
        }
        if has_type(FurnitureObject::COMPOSITE_ASSEMBLER) {
            self.progress.facts |= PROCESSOR_PLACED;
        }
        if powered_type(FurnitureObject::COMPOSITE_ASSEMBLER) {
            self.progress.facts |= PROCESSOR_POWERED;
        }
        self.progress.maximum_depth_decimetres = self
            .progress
            .maximum_depth_decimetres
            .max(depth_decimetres.saturating_neg().max(0) as u32);
        self.progress.iron_acquired =
            self.progress
                .iron_acquired
                .max(held_quantity(world, inventory, ItemId::IRON_ORE));
        self.progress.stone_extracted =
            self.progress
                .stone_extracted
                .max(held_quantity(world, inventory, ItemId::STONE_BLOCK));
        self.progress.asterite_acquired =
            self.progress
                .asterite_acquired
                .max(held_quantity(world, inventory, ItemId::ASTERITE));

        if self.is_unlocked(ContractId::HelpWanted) {
            let suitable = world
                .objects_of_type(FurnitureObject::PROCUREMENT_TERMINAL)
                .any(|terminal| {
                    assess_room(world, terminal.id()).is_some_and(|room| room.is_valid())
                });
            if suitable {
                self.progress.facts |= SUITABLE_ACCOMMODATION;
            }
        }
        if !world.specialists().is_empty() {
            self.progress.facts |= SPECIALIST_RECRUITED | SPECIALIST_MOVED_IN;
        }
    }

    fn observe_legacy_contract_progress(&mut self, contracts: &ContractBoard) {
        if let Some(contract) = contracts.active_contract(ContractId::BreakingGround)
            && let Some(progress) = contract.mine_progress()
            && progress.item == ItemId::STONE_BLOCK
        {
            self.progress.stone_extracted = self.progress.stone_extracted.max(progress.mined);
        }
        if let Some(contract) = contracts.active_contract(ContractId::FirstShipment) {
            if let Some(progress) = contract.export_progress()
                && progress.item == ItemId::STONE_BLOCK
            {
                self.progress.stone_exported = self.progress.stone_exported.max(progress.exported);
            }
            if let Some(progress) = contract.build_and_export_progress()
                && progress.item == ItemId::STONE_BLOCK
            {
                self.progress.stone_exported = self.progress.stone_exported.max(progress.exported);
            }
        }
    }

    fn unlock_missions(&self, contracts: &mut ContractBoard) {
        if !matches!(self.stage, TutorialStage::Active) {
            return;
        }
        self.add_if_missing(contracts, ContractId::BreakingGround);
        if self.is_complete(ContractId::BreakingGround) {
            self.add_if_missing(contracts, ContractId::SitePower);
            self.add_if_missing(contracts, ContractId::FirstShipment);
        }
        if self.is_complete(ContractId::SitePower) && self.is_complete(ContractId::FirstShipment) {
            self.add_if_missing(contracts, ContractId::Procurement);
        }
        if self.is_complete(ContractId::Procurement) {
            for id in EXPANSION_IDS {
                self.add_if_missing(contracts, id);
            }
        }
        if self.progress.iron_acquired > 0 {
            self.add_if_missing(contracts, ContractId::ValueAdded);
        }
        if self.progress.facts & (DRILL_PLACED | PROCESSOR_PLACED)
            == DRILL_PLACED | PROCESSOR_PLACED
        {
            self.add_if_missing(contracts, ContractId::HandsOff);
        }
        if [
            ContractId::IndustrialExtraction,
            ContractId::Prospecting,
            ContractId::IronAge,
            ContractId::GoingDown,
            ContractId::ValueAdded,
            ContractId::HandsOff,
        ]
        .into_iter()
        .all(|id| self.is_complete(id))
        {
            self.add_if_missing(contracts, ContractId::HelpWanted);
        }
        if self.is_complete(ContractId::GoingDown) {
            self.add_if_missing(contracts, ContractId::Depth100);
        }
        for pair in DEPTH_IDS.windows(2) {
            if self.is_complete(pair[0]) {
                self.add_if_missing(contracts, pair[1]);
            }
        }
        if self.is_complete(ContractId::Depth1000) {
            self.add_if_missing(contracts, ContractId::RecoverAsterite);
        }
    }

    fn add_if_missing(&self, contracts: &mut ContractBoard, id: ContractId) {
        if !self.is_complete(id) {
            contracts.add_active(mission(id));
        }
    }

    fn sync_progress(&self, contracts: &mut ContractBoard) {
        for id in INITIAL_IDS
            .into_iter()
            .chain(DEPTH_IDS)
            .chain([ContractId::RecoverAsterite])
        {
            let (completed, required) = self.mission_steps(id);
            contracts.set_program_progress(id, completed, required);
        }
    }

    fn capture_completions(&mut self, contracts: &ContractBoard) {
        for contract in contracts.active() {
            if let Some(id) = contract.id()
                && contract.program_progress().is_some()
                && contract.is_complete()
            {
                let bit = mission_bit(id);
                if self.progress.completed_missions & bit == 0
                    && let Some(index) = package_mission_index(id)
                {
                    self.progress.package_transmission_delays_ms[index] =
                        MISSION_TRANSMISSION_DELAY_MILLISECONDS;
                }
                self.progress.completed_missions |= bit;
            }
        }
    }

    fn mission_steps(&self, id: ContractId) -> (u16, u16) {
        let fact = |flag| u16::from(self.progress.facts & flag != 0);
        match id {
            ContractId::BreakingGround => (u16::from(self.progress.stone_extracted >= 20), 1),
            ContractId::SitePower => (fact(SOLAR_PLACED), 1),
            ContractId::FirstShipment => (
                fact(EXPORTER_POWERED) + u16::from(self.progress.stone_exported >= 20),
                2,
            ),
            ContractId::Procurement => (fact(TERMINAL_POWERED) + fact(PURCHASED_ITEM), 2),
            ContractId::IndustrialExtraction => (fact(DRILL_POWERED) + fact(DRILL_EXTRACTED), 2),
            ContractId::Prospecting => (fact(SCANNER_FOUND_IRON), 1),
            ContractId::IronAge => (u16::from(self.progress.iron_acquired >= 20), 1),
            ContractId::GoingDown => (u16::from(self.progress.maximum_depth_decimetres >= 500), 1),
            ContractId::ValueAdded => (
                fact(PROCESSOR_POWERED) + u16::from(self.progress.iron_processed >= 20),
                2,
            ),
            ContractId::HandsOff => (fact(AUTOMATIC_TRANSFER) + fact(AUTOMATIC_PROCESSING), 2),
            ContractId::HelpWanted => (
                fact(SUITABLE_ACCOMMODATION)
                    + fact(SPECIALIST_RECRUITED)
                    + fact(SPECIALIST_MOVED_IN),
                3,
            ),
            ContractId::Depth100 => self.depth_step(1_000),
            ContractId::Depth250 => self.depth_step(2_500),
            ContractId::Depth500 => self.depth_step(5_000),
            ContractId::Depth1000 => self.depth_step(10_000),
            ContractId::Depth2500 => self.depth_step(25_000),
            ContractId::Depth5000 => self.depth_step(50_000),
            ContractId::RecoverAsterite => (
                fact(SCANNER_FOUND_ASTERITE)
                    + u16::from(self.progress.asterite_acquired > 0)
                    + u16::from(self.progress.asterite_exported > 0),
                3,
            ),
        }
    }

    fn depth_step(&self, required_decimetres: u32) -> (u16, u16) {
        (
            u16::from(self.progress.maximum_depth_decimetres >= required_decimetres),
            1,
        )
    }

    fn advance_package_transmission_delays(
        &mut self,
        elapsed_seconds: f32,
        transmissions: &mut TransmissionLog,
        deliveries: &mut DeliverySystem,
    ) {
        let elapsed_ms = (elapsed_seconds * 1_000.0)
            .round()
            .clamp(0.0, u32::MAX as f32) as u32;
        for remaining in &mut self.progress.package_transmission_delays_ms {
            if *remaining > 1 {
                *remaining = remaining.saturating_sub(elapsed_ms).max(1);
            }
        }

        // Keep the persisted timer array at five entries for save compatibility;
        // slot 2 was the retired tutorial surveyor package.
        self.progress.package_transmission_delays_ms[2] = 0;
        let packages: &[(usize, ContractId, &[ItemId], &str, &str)] = &[
            (
                0,
                ContractId::BreakingGround,
                &[
                    ItemId::SOLAR_ARRAY,
                    ItemId::ORBITAL_EXPORT_LAUNCHER,
                    ItemId::PYLON,
                ],
                "SITE INFRASTRUCTURE PACKAGE",
                "Stone quota confirmed. A solar array, exporter, and pylon are inbound together.",
            ),
            (
                1,
                ContractId::FirstShipment,
                &[ItemId::PROCUREMENT_TERMINAL],
                "PROCUREMENT PACKAGE",
                "First shipment confirmed. A field procurement terminal is inbound.",
            ),
            (
                3,
                ContractId::IronAge,
                &[ItemId::COMPOSITE_ASSEMBLER],
                "PROCESSING PACKAGE",
                "Iron supply confirmed. A processor is inbound.",
            ),
            (
                4,
                ContractId::ValueAdded,
                &CONVEYOR_PACKAGE,
                "AUTOMATION PACKAGE",
                "Processing confirmed. Cargo conveyors are inbound.",
            ),
        ];
        for &(index, id, items, subject, body) in packages {
            let bit = mission_bit(id);
            if self.is_complete(id)
                && self.progress.issued_packages & bit == 0
                && self.progress.package_transmission_delays_ms[index] == 1
                && deliveries
                    .schedule_batch(items, PROGRAM_DELIVERY_DELAY_SECONDS)
                    .is_ok()
            {
                self.progress.issued_packages |= bit;
                self.progress.package_transmission_delays_ms[index] = 0;
                transmissions.receive(Transmission::new(
                    "DEEPTEK PROSPECTOR PROGRAM",
                    subject,
                    body,
                ));
            }
        }
    }

    fn is_complete(self, id: ContractId) -> bool {
        self.progress.completed_missions & mission_bit(id) != 0
    }

    fn is_unlocked(self, id: ContractId) -> bool {
        self.is_complete(id)
            || matches!(id, ContractId::HelpWanted)
                && self.progress.completed_missions & mission_bit(ContractId::HandsOff) != 0
    }

    pub(crate) fn saved_state(self) -> (u8, f32, SavedProspectorProgress) {
        let (stage, remaining) = match self.stage {
            TutorialStage::Disabled => (0, 0.0),
            TutorialStage::BriefingPending(remaining) => (1, remaining),
            TutorialStage::Active => (2, 0.0),
        };
        (stage, remaining, self.progress)
    }

    pub(crate) fn from_saved(
        stage: u8,
        remaining: f32,
        progress: Option<SavedProspectorProgress>,
    ) -> Option<Self> {
        if !remaining.is_finite() || remaining < 0.0 {
            return None;
        }
        let stage_code = stage;
        let stage = match stage {
            0 if remaining == 0.0 => TutorialStage::Disabled,
            1 => TutorialStage::BriefingPending(remaining),
            // Stages 2-5 were used by the original linear tutorial. Their timer
            // no longer has meaning, but accepting it preserves existing worlds.
            2..=5 => TutorialStage::Active,
            _ => return None,
        };
        let mut progress = progress.unwrap_or_default();
        if progress == SavedProspectorProgress::default() {
            progress.completed_missions = match stage {
                TutorialStage::Active if stage_code == 3 => mission_bit(ContractId::BreakingGround),
                TutorialStage::Active if stage_code == 5 => {
                    mission_bit(ContractId::BreakingGround)
                        | mission_bit(ContractId::SitePower)
                        | mission_bit(ContractId::FirstShipment)
                }
                _ => 0,
            };
        }
        if progress
            .package_transmission_delays_ms
            .iter()
            .any(|&remaining| remaining > MISSION_TRANSMISSION_DELAY_MILLISECONDS)
        {
            return None;
        }
        for id in [
            ContractId::BreakingGround,
            ContractId::FirstShipment,
            ContractId::IronAge,
            ContractId::ValueAdded,
        ] {
            let bit = mission_bit(id);
            let index = package_mission_index(id).expect("listed missions have packages");
            if progress.completed_missions & bit != 0
                && progress.issued_packages & bit == 0
                && progress.package_transmission_delays_ms[index] == 0
            {
                progress.package_transmission_delays_ms[index] =
                    MISSION_TRANSMISSION_DELAY_MILLISECONDS;
            }
        }
        Some(Self { stage, progress })
    }
}

const fn package_mission_index(id: ContractId) -> Option<usize> {
    match id {
        ContractId::BreakingGround => Some(0),
        ContractId::FirstShipment => Some(1),
        ContractId::IronAge => Some(3),
        ContractId::ValueAdded => Some(4),
        _ => None,
    }
}

fn held_quantity(world: &World, inventory: &Inventory, item: ItemId) -> u64 {
    let personal = inventory
        .slots()
        .iter()
        .flatten()
        .filter(|stack| stack.item() == item)
        .map(|stack| u64::from(stack.quantity()))
        .sum::<u64>();
    BUILT_IN_FURNITURE
        .iter()
        .filter(|definition| definition.interaction().container_slots().is_some())
        .flat_map(|definition| world.objects_of_type(definition.object_type()))
        .fold(personal, |total, object| {
            total.saturating_add(
                world
                    .container(object.id())
                    .into_iter()
                    .flat_map(|container| container.slots().iter().flatten())
                    .filter(|stack| stack.item() == item)
                    .map(|stack| u64::from(stack.quantity()))
                    .sum::<u64>(),
            )
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TutorialMissionDefinition {
    pub id: ContractId,
    pub text: &'static str,
    pub reward: u64,
    pub experience: u32,
    pub steps: u16,
    /// Stable bit used by saved tutorial progress. Never reuse an existing value.
    pub progress_bit: u8,
}

macro_rules! tutorial_missions {
    ($(($bit:expr, $id:ident, $text:literal, $reward:expr, $experience:expr, $steps:expr)),+ $(,)?) => {
        pub const TUTORIAL_MISSIONS: &[TutorialMissionDefinition] = &[
            $(TutorialMissionDefinition {
                id: ContractId::$id,
                text: $text,
                reward: $reward,
                experience: $experience,
                steps: $steps,
                progress_bit: $bit,
            }),+
        ];
    };
}

tutorial_missions![
    (
        0,
        BreakingGround,
        "01 - BREAKING GROUND: EXTRACT 20 STONE",
        250,
        50,
        1
    ),
    (
        1,
        SitePower,
        "02 - SITE POWER: PLACE A SOLAR PANEL",
        500,
        75,
        1
    ),
    (
        2,
        FirstShipment,
        "03 - FIRST SHIPMENT: POWER EXPORTER; EXPORT 20 STONE",
        1_000,
        100,
        2
    ),
    (
        3,
        Procurement,
        "04 - PROCUREMENT: POWER TERMINAL; PURCHASE AN ITEM",
        750,
        100,
        2
    ),
    (
        4,
        IndustrialExtraction,
        "05 - INDUSTRIAL EXTRACTION: POWER DRILL; EXTRACT RESOURCE",
        1_500,
        150,
        2
    ),
    (
        5,
        Prospecting,
        "06 - PROSPECTING: SCAN AN IRON DEPOSIT",
        1_000,
        125,
        1
    ),
    (
        6,
        IronAge,
        "07 - IRON AGE: ACQUIRE 20 IRON ORE",
        1_500,
        150,
        1
    ),
    (
        7,
        GoingDown,
        "08 - GOING DOWN: REACH 50 M DEPTH",
        1_000,
        125,
        1
    ),
    (
        8,
        ValueAdded,
        "09 - VALUE ADDED: POWER PROCESSOR; PROCESS 20 IRON",
        2_500,
        200,
        2
    ),
    (
        9,
        HandsOff,
        "10 - HANDS OFF: AUTO-TRANSFER; AUTO-PROCESS",
        3_000,
        250,
        2
    ),
    (
        10,
        HelpWanted,
        "11 - HELP WANTED: HOUSE; RECRUIT; MOVE IN SPECIALIST",
        4_000,
        300,
        3
    ),
    (11, Depth100, "DEPTH MILESTONE: REACH 100 M", 1_500, 100, 1),
    (12, Depth250, "DEPTH MILESTONE: REACH 250 M", 2_500, 125, 1),
    (13, Depth500, "DEPTH MILESTONE: REACH 500 M", 4_000, 150, 1),
    (
        14,
        Depth1000,
        "DEPTH MILESTONE: REACH 1,000 M",
        7_500,
        200,
        1
    ),
    (
        15,
        Depth2500,
        "DEPTH MILESTONE: REACH 2,500 M",
        12_000,
        250,
        1
    ),
    (
        16,
        Depth5000,
        "DEPTH MILESTONE: REACH 5,000 M",
        20_000,
        350,
        1
    ),
    (
        17,
        RecoverAsterite,
        "PRIMARY: LOCATE, EXTRACT, AND EXPORT ASTERITE",
        50_000,
        500,
        3
    ),
];

pub fn tutorial_mission_definition(id: ContractId) -> Option<&'static TutorialMissionDefinition> {
    TUTORIAL_MISSIONS
        .iter()
        .find(|definition| definition.id == id)
}

fn mission_bit(id: ContractId) -> u64 {
    tutorial_mission_definition(id).map_or(0, |definition| 1_u64 << definition.progress_bit)
}

fn mission(id: ContractId) -> Contract {
    let definition = tutorial_mission_definition(id).expect("tutorial mission ID is registered");
    Contract::program_mission(
        id,
        definition.text,
        definition.reward,
        definition.experience,
        definition.steps,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ItemRegistry;
    use std::collections::HashSet;

    fn program_world() -> (World, Inventory, PowerSystem) {
        (
            World::empty(128, 200, 1).unwrap(),
            Inventory::starter(&ItemRegistry::with_built_ins()),
            PowerSystem::new(),
        )
    }

    #[test]
    fn mission_definitions_are_complete_and_save_stable() {
        let expected = INITIAL_IDS
            .into_iter()
            .chain(DEPTH_IDS)
            .chain([ContractId::RecoverAsterite])
            .collect::<HashSet<_>>();
        let ids = TUTORIAL_MISSIONS
            .iter()
            .map(|definition| definition.id)
            .collect::<HashSet<_>>();
        let bits = TUTORIAL_MISSIONS
            .iter()
            .map(|definition| definition.progress_bit)
            .collect::<HashSet<_>>();

        assert_eq!(ids, expected);
        assert_eq!(bits.len(), TUTORIAL_MISSIONS.len());
        assert!(bits.iter().all(|&bit| bit < 64));
        for definition in TUTORIAL_MISSIONS {
            let contract = mission(definition.id);
            assert_eq!(contract.requirement, definition.text);
            assert_eq!(contract.reward, definition.reward);
            assert_eq!(contract.experience_reward, definition.experience);
            assert_eq!(
                contract.program_progress().unwrap().required,
                definition.steps
            );
        }
    }

    #[test]
    fn briefing_adds_breaking_ground_after_five_seconds() {
        let (world, inventory, power) = program_world();
        let mut tutorial = TutorialProgram::for_new_world();
        let mut contracts = ContractBoard::with_built_ins();
        let mut transmissions = TransmissionLog::default();
        let mut deliveries = DeliverySystem::default();
        tutorial.update(
            5.0,
            &mut contracts,
            &mut transmissions,
            &mut deliveries,
            &world,
            &power,
            &inventory,
            0,
        );
        assert!(contracts.contains_active(ContractId::BreakingGround));
        assert_eq!(transmissions.history().len(), 1);
    }

    #[test]
    fn extraction_before_briefing_counts_retroactively() {
        let (world, inventory, power) = program_world();
        let mut tutorial = TutorialProgram::for_new_world();
        tutorial.record_extraction(ItemId::STONE_BLOCK, 20);
        let mut contracts = ContractBoard::with_built_ins();
        let mut transmissions = TransmissionLog::default();
        let mut deliveries = DeliverySystem::default();
        tutorial.update(
            5.0,
            &mut contracts,
            &mut transmissions,
            &mut deliveries,
            &world,
            &power,
            &inventory,
            0,
        );
        assert!(
            contracts
                .active_contract(ContractId::BreakingGround)
                .is_some_and(Contract::is_complete)
        );
        assert!(contracts.contains_active(ContractId::SitePower));
        assert!(contracts.contains_active(ContractId::FirstShipment));
        assert!(!contracts.contains_active(ContractId::Procurement));
    }

    #[test]
    fn initial_setup_unlocks_four_parallel_missions() {
        let (world, inventory, power) = program_world();
        let mut tutorial = TutorialProgram::for_new_world();
        tutorial.stage = TutorialStage::Active;
        tutorial.progress.completed_missions = mission_bit(ContractId::BreakingGround)
            | mission_bit(ContractId::SitePower)
            | mission_bit(ContractId::FirstShipment)
            | mission_bit(ContractId::Procurement);
        let mut contracts = ContractBoard::with_built_ins();
        let mut transmissions = TransmissionLog::default();
        let mut deliveries = DeliverySystem::default();
        tutorial.update(
            0.0,
            &mut contracts,
            &mut transmissions,
            &mut deliveries,
            &world,
            &power,
            &inventory,
            0,
        );
        for id in EXPANSION_IDS {
            assert!(contracts.contains_active(id));
        }
    }

    #[test]
    fn completion_transmission_waits_four_seconds() {
        let (world, inventory, power) = program_world();
        let mut tutorial = TutorialProgram::for_new_world();
        tutorial.stage = TutorialStage::Active;
        tutorial.record_extraction(ItemId::STONE_BLOCK, 20);
        let mut contracts = ContractBoard::with_built_ins();
        let mut transmissions = TransmissionLog::default();
        let mut deliveries = DeliverySystem::default();

        tutorial.update(
            0.0,
            &mut contracts,
            &mut transmissions,
            &mut deliveries,
            &world,
            &power,
            &inventory,
            0,
        );
        assert!(
            contracts
                .active_contract(ContractId::BreakingGround)
                .unwrap()
                .is_complete()
        );
        assert!(transmissions.history().is_empty());
        assert_eq!(deliveries.pending_count(), 0);

        tutorial.update(
            3.9,
            &mut contracts,
            &mut transmissions,
            &mut deliveries,
            &world,
            &power,
            &inventory,
            0,
        );
        assert!(transmissions.history().is_empty());
        tutorial.update(
            0.1,
            &mut contracts,
            &mut transmissions,
            &mut deliveries,
            &world,
            &power,
            &inventory,
            0,
        );
        assert_eq!(transmissions.history().len(), 1);
        assert_eq!(deliveries.pending_count(), 3);
    }

    #[test]
    fn procurement_completion_does_not_schedule_the_retired_surveyor_package() {
        let (world, inventory, power) = program_world();
        let progress = SavedProspectorProgress {
            completed_missions: mission_bit(ContractId::Procurement),
            package_transmission_delays_ms: [0, 0, 1, 0, 0],
            ..SavedProspectorProgress::default()
        };
        let mut tutorial = TutorialProgram {
            stage: TutorialStage::Active,
            progress,
        };
        let mut contracts = ContractBoard::with_built_ins();
        let mut transmissions = TransmissionLog::default();
        let mut deliveries = DeliverySystem::default();

        tutorial.update(
            0.0,
            &mut contracts,
            &mut transmissions,
            &mut deliveries,
            &world,
            &power,
            &inventory,
            0,
        );

        assert_eq!(deliveries.pending_count(), 0);
        assert!(transmissions.history().is_empty());
        assert_eq!(tutorial.progress.package_transmission_delays_ms[2], 0);
    }
}
