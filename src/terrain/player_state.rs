use crate::{
    ContractBoard, CorporationProgress, DeliverySystem, Inventory, ItemStack, TransmissionLog,
    TutorialProgram,
};

/// World-owned player data that must not leak between save files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerState {
    health_current: u16,
    health_maximum: u16,
    inventory: Inventory,
    cursor_stack: Option<ItemStack>,
    corporation_progress: CorporationProgress,
    contract_board: ContractBoard,
    transmission_log: TransmissionLog,
    tutorial_program: TutorialProgram,
    delivery_system: DeliverySystem,
}

impl PlayerState {
    pub fn new(health_current: u16, health_maximum: u16, inventory: Inventory) -> Option<Self> {
        (health_maximum > 0 && health_current <= health_maximum).then_some(Self {
            health_current,
            health_maximum,
            inventory,
            cursor_stack: None,
            corporation_progress: CorporationProgress::default(),
            contract_board: ContractBoard::with_built_ins(),
            transmission_log: TransmissionLog::default(),
            tutorial_program: TutorialProgram::default(),
            delivery_system: DeliverySystem::default(),
        })
    }

    pub const fn health_current(&self) -> u16 {
        self.health_current
    }

    pub const fn health_maximum(&self) -> u16 {
        self.health_maximum
    }

    pub const fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    pub const fn cursor_stack(&self) -> Option<ItemStack> {
        self.cursor_stack
    }

    pub const fn with_cursor_stack(mut self, cursor_stack: Option<ItemStack>) -> Self {
        self.cursor_stack = cursor_stack;
        self
    }

    pub const fn corporation_progress(&self) -> CorporationProgress {
        self.corporation_progress
    }

    pub const fn with_corporation_progress(
        mut self,
        corporation_progress: CorporationProgress,
    ) -> Self {
        self.corporation_progress = corporation_progress;
        self
    }

    pub const fn contract_board(&self) -> &ContractBoard {
        &self.contract_board
    }

    pub const fn transmission_log(&self) -> &TransmissionLog {
        &self.transmission_log
    }

    pub const fn tutorial_program(&self) -> TutorialProgram {
        self.tutorial_program
    }

    pub const fn delivery_system(&self) -> &DeliverySystem {
        &self.delivery_system
    }

    pub fn with_mission_state(
        mut self,
        contract_board: ContractBoard,
        transmission_log: TransmissionLog,
        tutorial_program: TutorialProgram,
        delivery_system: DeliverySystem,
    ) -> Self {
        self.contract_board = contract_board;
        self.transmission_log = transmission_log;
        self.tutorial_program = tutorial_program;
        self.delivery_system = delivery_system;
        self
    }
}
