use crate::{ItemId, ObjectTypeId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContractCompany {
    DeepTekIndustries,
    VanguardDefence,
    AstraSurveyCorp,
}

pub const MAX_CORPORATION_LEVEL: u8 = 5;
pub const CORPORATION_LEVEL_THRESHOLDS: [u32; MAX_CORPORATION_LEVEL as usize + 1] =
    [0, 100, 250, 500, 900, 1_500];

impl ContractCompany {
    pub const ALL: [Self; 3] = [
        Self::DeepTekIndustries,
        Self::VanguardDefence,
        Self::AstraSurveyCorp,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::DeepTekIndustries => 0,
            Self::VanguardDefence => 1,
            Self::AstraSurveyCorp => 2,
        }
    }

    pub const fn short_name(self) -> &'static str {
        match self {
            Self::DeepTekIndustries => "DEEPTEK",
            Self::VanguardDefence => "VANGUARD",
            Self::AstraSurveyCorp => "ASTRA",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CorporationProgress {
    experience: [u32; ContractCompany::ALL.len()],
}

impl CorporationProgress {
    pub fn from_experience(experience: [u32; ContractCompany::ALL.len()]) -> Self {
        let maximum = CORPORATION_LEVEL_THRESHOLDS[MAX_CORPORATION_LEVEL as usize];
        Self {
            experience: [
                experience[0].min(maximum),
                experience[1].min(maximum),
                experience[2].min(maximum),
            ],
        }
    }

    pub const fn experience(self, company: ContractCompany) -> u32 {
        self.experience[company.index()]
    }

    pub fn award(&mut self, company: ContractCompany, amount: u32) -> u32 {
        let index = company.index();
        let maximum = CORPORATION_LEVEL_THRESHOLDS[MAX_CORPORATION_LEVEL as usize];
        let previous = self.experience[index];
        self.experience[index] = previous.saturating_add(amount).min(maximum);
        self.experience[index] - previous
    }

    pub fn level(self, company: ContractCompany) -> u8 {
        CORPORATION_LEVEL_THRESHOLDS
            .iter()
            .rposition(|&threshold| self.experience(company) >= threshold)
            .unwrap_or(0) as u8
    }

    pub fn next_level_experience(self, company: ContractCompany) -> Option<u32> {
        let level = self.level(company);
        (level < MAX_CORPORATION_LEVEL)
            .then(|| CORPORATION_LEVEL_THRESHOLDS[usize::from(level) + 1])
    }

    pub const fn all_experience(self) -> [u32; ContractCompany::ALL.len()] {
        self.experience
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContractId {
    BreakingGround,
    FirstShipment,
    SitePower,
    Procurement,
    IndustrialExtraction,
    Prospecting,
    IronAge,
    GoingDown,
    ValueAdded,
    HandsOff,
    HelpWanted,
    Depth100,
    Depth250,
    Depth500,
    Depth1000,
    Depth2500,
    Depth5000,
    RecoverAsterite,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ContractObjective {
    ExportItems {
        item: ItemId,
        exported: u64,
        required: u64,
    },
    MineItems {
        item: ItemId,
        mined: u64,
        required: u64,
    },
    BuildAndExport {
        required_objects: Vec<ObjectTypeId>,
        placed_objects: Vec<ObjectTypeId>,
        item: ItemId,
        exported: u64,
        required: u64,
    },
    Program {
        completed: u16,
        required: u16,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SavedContractObjective {
    None,
    ExportItems {
        item: ItemId,
        exported: u64,
        required: u64,
    },
    MineItems {
        item: ItemId,
        mined: u64,
        required: u64,
    },
    BuildAndExport {
        required_objects: Vec<ObjectTypeId>,
        placed_objects: Vec<ObjectTypeId>,
        item: ItemId,
        exported: u64,
        required: u64,
    },
    Program {
        completed: u16,
        required: u16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportContractProgress {
    pub item: ItemId,
    pub exported: u64,
    pub required: u64,
}

impl ExportContractProgress {
    pub const fn is_complete(self) -> bool {
        self.exported >= self.required
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MineContractProgress {
    pub item: ItemId,
    pub mined: u64,
    pub required: u64,
}

impl MineContractProgress {
    pub const fn is_complete(self) -> bool {
        self.mined >= self.required
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildAndExportContractProgress {
    pub placed: usize,
    pub required_placements: usize,
    pub item: ItemId,
    pub exported: u64,
    pub required_exports: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramContractProgress {
    pub completed: u16,
    pub required: u16,
}

impl ProgramContractProgress {
    pub const fn is_complete(self) -> bool {
        self.completed >= self.required
    }
}

impl BuildAndExportContractProgress {
    pub const fn is_complete(self) -> bool {
        self.placed >= self.required_placements && self.exported >= self.required_exports
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContractProgressResult {
    pub contributed: u64,
    pub completed_contracts: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContractExportResult {
    /// Number of exported items assigned to matching unfinished contracts.
    pub contributed: u64,
    /// Number of contracts made ready for manual reward collection.
    pub completed_contracts: usize,
}

pub const MAX_ACTIVE_CONTRACTS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptContractError {
    InvalidContract,
    ActiveLimitReached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimContractError {
    InvalidContract,
    Incomplete,
}

impl ContractCompany {
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::DeepTekIndustries => "DEEPTEK INDUSTRIES",
            Self::VanguardDefence => "VANGUARD DEFENCE",
            Self::AstraSurveyCorp => "ASTRA SURVEY CORP",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    id: Option<ContractId>,
    pub requirement: String,
    pub reward: u64,
    pub company: ContractCompany,
    pub experience_reward: u32,
    objective: Option<ContractObjective>,
}

impl Contract {
    pub fn new(requirement: impl Into<String>, reward: u64, company: ContractCompany) -> Self {
        Self {
            id: None,
            requirement: requirement.into(),
            reward,
            company,
            experience_reward: 100,
            objective: None,
        }
    }

    pub fn export_items(
        requirement: impl Into<String>,
        reward: u64,
        company: ContractCompany,
        item: ItemId,
        required: u64,
    ) -> Self {
        assert!(required > 0, "export contracts require at least one item");
        Self {
            id: None,
            requirement: requirement.into(),
            reward,
            company,
            experience_reward: 100,
            objective: Some(ContractObjective::ExportItems {
                item,
                exported: 0,
                required,
            }),
        }
    }

    pub fn mine_items(
        requirement: impl Into<String>,
        reward: u64,
        company: ContractCompany,
        item: ItemId,
        required: u64,
    ) -> Self {
        assert!(required > 0, "mining contracts require at least one item");
        Self {
            id: None,
            requirement: requirement.into(),
            reward,
            company,
            experience_reward: 100,
            objective: Some(ContractObjective::MineItems {
                item,
                mined: 0,
                required,
            }),
        }
    }

    pub fn build_and_export(
        requirement: impl Into<String>,
        reward: u64,
        company: ContractCompany,
        required_objects: Vec<ObjectTypeId>,
        item: ItemId,
        required_exports: u64,
    ) -> Self {
        assert!(
            !required_objects.is_empty(),
            "build-and-export contracts require at least one object"
        );
        assert!(
            required_exports > 0,
            "build-and-export contracts require at least one exported item"
        );
        Self {
            id: None,
            requirement: requirement.into(),
            reward,
            company,
            experience_reward: 100,
            objective: Some(ContractObjective::BuildAndExport {
                required_objects,
                placed_objects: Vec::new(),
                item,
                exported: 0,
                required: required_exports,
            }),
        }
    }

    pub fn program_mission(
        id: ContractId,
        requirement: impl Into<String>,
        reward: u64,
        experience_reward: u32,
        required_steps: u16,
    ) -> Self {
        assert!(
            required_steps > 0,
            "program missions require at least one step"
        );
        Self {
            id: Some(id),
            requirement: requirement.into(),
            reward,
            company: ContractCompany::DeepTekIndustries,
            experience_reward,
            objective: Some(ContractObjective::Program {
                completed: 0,
                required: required_steps,
            }),
        }
    }

    pub const fn with_id(mut self, id: ContractId) -> Self {
        self.id = Some(id);
        self
    }

    pub const fn id(&self) -> Option<ContractId> {
        self.id
    }

    pub const fn with_experience_reward(mut self, experience_reward: u32) -> Self {
        self.experience_reward = experience_reward;
        self
    }

    pub fn export_progress(&self) -> Option<ExportContractProgress> {
        match &self.objective {
            Some(ContractObjective::ExportItems {
                item,
                exported,
                required,
            }) => Some(ExportContractProgress {
                item: *item,
                exported: *exported,
                required: *required,
            }),
            None => None,
            Some(
                ContractObjective::MineItems { .. }
                | ContractObjective::BuildAndExport { .. }
                | ContractObjective::Program { .. },
            ) => None,
        }
    }

    pub fn mine_progress(&self) -> Option<MineContractProgress> {
        match &self.objective {
            Some(ContractObjective::MineItems {
                item,
                mined,
                required,
            }) => Some(MineContractProgress {
                item: *item,
                mined: *mined,
                required: *required,
            }),
            _ => None,
        }
    }

    pub fn build_and_export_progress(&self) -> Option<BuildAndExportContractProgress> {
        match &self.objective {
            Some(ContractObjective::BuildAndExport {
                required_objects,
                placed_objects,
                item,
                exported,
                required,
            }) => Some(BuildAndExportContractProgress {
                placed: placed_objects.len(),
                required_placements: required_objects.len(),
                item: *item,
                exported: *exported,
                required_exports: *required,
            }),
            _ => None,
        }
    }

    pub fn program_progress(&self) -> Option<ProgramContractProgress> {
        match self.objective {
            Some(ContractObjective::Program {
                completed,
                required,
            }) => Some(ProgramContractProgress {
                completed,
                required,
            }),
            _ => None,
        }
    }

    pub fn is_complete(&self) -> bool {
        if let Some(progress) = self.export_progress() {
            progress.is_complete()
        } else if let Some(progress) = self.mine_progress() {
            progress.is_complete()
        } else if let Some(progress) = self.build_and_export_progress() {
            progress.is_complete()
        } else if let Some(progress) = self.program_progress() {
            progress.is_complete()
        } else {
            false
        }
    }

    pub(crate) fn saved_objective(&self) -> SavedContractObjective {
        match &self.objective {
            None => SavedContractObjective::None,
            Some(ContractObjective::ExportItems {
                item,
                exported,
                required,
            }) => SavedContractObjective::ExportItems {
                item: *item,
                exported: *exported,
                required: *required,
            },
            Some(ContractObjective::MineItems {
                item,
                mined,
                required,
            }) => SavedContractObjective::MineItems {
                item: *item,
                mined: *mined,
                required: *required,
            },
            Some(ContractObjective::BuildAndExport {
                required_objects,
                placed_objects,
                item,
                exported,
                required,
            }) => SavedContractObjective::BuildAndExport {
                required_objects: required_objects.clone(),
                placed_objects: placed_objects.clone(),
                item: *item,
                exported: *exported,
                required: *required,
            },
            Some(ContractObjective::Program {
                completed,
                required,
            }) => SavedContractObjective::Program {
                completed: *completed,
                required: *required,
            },
        }
    }

    pub(crate) fn from_saved(
        id: Option<ContractId>,
        requirement: String,
        reward: u64,
        company: ContractCompany,
        experience_reward: u32,
        objective: SavedContractObjective,
    ) -> Option<Self> {
        let objective = match objective {
            SavedContractObjective::None => None,
            SavedContractObjective::ExportItems {
                item,
                exported,
                required,
            } if required > 0 && exported <= required => Some(ContractObjective::ExportItems {
                item,
                exported,
                required,
            }),
            SavedContractObjective::MineItems {
                item,
                mined,
                required,
            } if required > 0 && mined <= required => Some(ContractObjective::MineItems {
                item,
                mined,
                required,
            }),
            SavedContractObjective::BuildAndExport {
                required_objects,
                placed_objects,
                item,
                exported,
                required,
            } if !required_objects.is_empty()
                && placed_objects.len() <= required_objects.len()
                && required > 0
                && exported <= required =>
            {
                Some(ContractObjective::BuildAndExport {
                    required_objects,
                    placed_objects,
                    item,
                    exported,
                    required,
                })
            }
            SavedContractObjective::Program {
                completed,
                required,
            } if required > 0 && completed <= required => Some(ContractObjective::Program {
                completed,
                required,
            }),
            _ => return None,
        };
        Some(Self {
            id,
            requirement,
            reward,
            company,
            experience_reward,
            objective,
        })
    }

    fn contribute_export(&mut self, item: ItemId, available: u64) -> ContractExportResult {
        let (required_item, exported, required) = match self.objective.as_mut() {
            Some(ContractObjective::ExportItems {
                item,
                exported,
                required,
            })
            | Some(ContractObjective::BuildAndExport {
                item,
                exported,
                required,
                ..
            }) => (item, exported, required),
            _ => return ContractExportResult::default(),
        };
        if *required_item != item || *exported >= *required || available == 0 {
            return ContractExportResult::default();
        }

        let contributed = available.min(*required - *exported);
        *exported += contributed;
        ContractExportResult {
            contributed,
            completed_contracts: usize::from(*exported == *required),
        }
    }

    fn contribute_mined(&mut self, item: ItemId, available: u64) -> ContractProgressResult {
        let Some(ContractObjective::MineItems {
            item: required_item,
            mined,
            required,
        }) = self.objective.as_mut()
        else {
            return ContractProgressResult::default();
        };
        if *required_item != item || *mined >= *required || available == 0 {
            return ContractProgressResult::default();
        }
        let contributed = available.min(*required - *mined);
        *mined += contributed;
        ContractProgressResult {
            contributed,
            completed_contracts: usize::from(*mined == *required),
        }
    }

    fn contribute_placement(&mut self, object_type: ObjectTypeId) -> ContractProgressResult {
        let was_complete = self.is_complete();
        let Some(ContractObjective::BuildAndExport {
            required_objects,
            placed_objects,
            ..
        }) = self.objective.as_mut()
        else {
            return ContractProgressResult::default();
        };
        let required_count = required_objects
            .iter()
            .filter(|&&required| required == object_type)
            .count();
        let placed_count = placed_objects
            .iter()
            .filter(|&&placed| placed == object_type)
            .count();
        if placed_count >= required_count {
            return ContractProgressResult::default();
        }
        placed_objects.push(object_type);
        ContractProgressResult {
            contributed: 1,
            completed_contracts: usize::from(!was_complete && self.is_complete()),
        }
    }

    fn set_program_progress(&mut self, completed: u16, required: u16) -> bool {
        let Some(ContractObjective::Program {
            completed: current,
            required: expected,
        }) = self.objective.as_mut()
        else {
            return false;
        };
        if *expected != required {
            return false;
        }
        let was_complete = *current >= *expected;
        *current = completed.min(required).max(*current);
        !was_complete && *current >= *expected
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractBoard {
    available: Vec<Contract>,
    active: Vec<Contract>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractReward {
    pub money: u64,
    pub company: ContractCompany,
    pub experience: u32,
}

impl ContractBoard {
    pub fn new(available: Vec<Contract>) -> Self {
        Self {
            available,
            active: Vec::new(),
        }
    }

    pub fn with_built_ins() -> Self {
        Self::new(built_in_contracts())
    }

    pub fn available(&self) -> &[Contract] {
        &self.available
    }

    pub fn active(&self) -> &[Contract] {
        &self.active
    }

    pub(crate) fn from_saved(available: Vec<Contract>, active: Vec<Contract>) -> Option<Self> {
        let ids: Vec<_> = active.iter().filter_map(Contract::id).collect();
        if ids
            .iter()
            .enumerate()
            .any(|(index, id)| ids[index + 1..].contains(id))
        {
            return None;
        }
        Some(Self { available, active })
    }

    pub fn add_active(&mut self, contract: Contract) {
        if let Some(id) = contract.id()
            && let Some(existing) = self
                .active
                .iter_mut()
                .find(|active| active.id() == Some(id))
        {
            if existing.program_progress().is_none() && contract.program_progress().is_some() {
                *existing = contract;
            }
            return;
        }
        self.active.insert(0, contract);
    }

    pub fn active_contract(&self, id: ContractId) -> Option<&Contract> {
        self.active
            .iter()
            .find(|contract| contract.id() == Some(id))
    }

    pub fn contains_active(&self, id: ContractId) -> bool {
        self.active_contract(id).is_some()
    }

    pub fn set_program_progress(&mut self, id: ContractId, completed: u16, required: u16) -> bool {
        self.active
            .iter_mut()
            .find(|contract| contract.id() == Some(id))
            .is_some_and(|contract| contract.set_program_progress(completed, required))
    }

    pub fn accept(&mut self, available_index: usize) -> Result<(), AcceptContractError> {
        if available_index >= self.available.len() {
            return Err(AcceptContractError::InvalidContract);
        }
        if self
            .active
            .iter()
            .filter(|contract| contract.id().is_none())
            .count()
            >= MAX_ACTIVE_CONTRACTS
        {
            return Err(AcceptContractError::ActiveLimitReached);
        }
        self.active.push(self.available.remove(available_index));
        Ok(())
    }

    pub fn apply_export(&mut self, item: ItemId, quantity: u64) -> ContractExportResult {
        apply_export_to_contracts(&mut self.active, item, quantity)
    }

    pub fn apply_mined(&mut self, item: ItemId, quantity: u64) -> ContractProgressResult {
        apply_mined_to_contracts(&mut self.active, item, quantity)
    }

    pub fn apply_placement(&mut self, object_type: ObjectTypeId) -> ContractProgressResult {
        let mut result = ContractProgressResult::default();
        for contract in &mut self.active {
            let contribution = contract.contribute_placement(object_type);
            result.contributed = result.contributed.saturating_add(contribution.contributed);
            result.completed_contracts = result
                .completed_contracts
                .saturating_add(contribution.completed_contracts);
        }
        result
    }

    pub fn claim_reward(
        &mut self,
        active_index: usize,
    ) -> Result<ContractReward, ClaimContractError> {
        let contract = self
            .active
            .get(active_index)
            .ok_or(ClaimContractError::InvalidContract)?;
        if !contract.is_complete() {
            return Err(ClaimContractError::Incomplete);
        }
        let contract = self.active.remove(active_index);
        Ok(ContractReward {
            money: contract.reward,
            company: contract.company,
            experience: contract.experience_reward,
        })
    }
}

pub fn apply_mined_to_contracts(
    contracts: &mut [Contract],
    item: ItemId,
    quantity: u64,
) -> ContractProgressResult {
    let mut result = ContractProgressResult::default();
    let mut remaining = quantity;
    for contract in contracts {
        let contribution = contract.contribute_mined(item, remaining);
        result.contributed = result.contributed.saturating_add(contribution.contributed);
        result.completed_contracts = result
            .completed_contracts
            .saturating_add(contribution.completed_contracts);
        remaining -= contribution.contributed;
        if remaining == 0 {
            break;
        }
    }
    result
}

/// Assigns one shipment across matching contracts in display order. Each item
/// contributes at most once, even when several contracts request the same item.
pub fn apply_export_to_contracts(
    contracts: &mut [Contract],
    item: ItemId,
    quantity: u64,
) -> ContractExportResult {
    let mut result = ContractExportResult::default();
    let mut remaining = quantity;
    for contract in contracts {
        let contribution = contract.contribute_export(item, remaining);
        result.contributed = result.contributed.saturating_add(contribution.contributed);
        result.completed_contracts = result
            .completed_contracts
            .saturating_add(contribution.completed_contracts);
        remaining -= contribution.contributed;
        if remaining == 0 {
            break;
        }
    }
    result
}

pub fn built_in_contracts() -> Vec<Contract> {
    vec![
        Contract::export_items(
            "DELIVER 50 STONE BLOCKS",
            1_200,
            ContractCompany::DeepTekIndustries,
            ItemId::STONE_BLOCK,
            50,
        )
        .with_experience_reward(80),
        Contract::export_items(
            "DELIVER 20 HARDENED COMPOSITE",
            1_800,
            ContractCompany::VanguardDefence,
            ItemId::HARDENED_COMPOSITE,
            20,
        )
        .with_experience_reward(180),
        Contract::export_items(
            "DELIVER 10 RED LIGHTS",
            700,
            ContractCompany::AstraSurveyCorp,
            ItemId::RED_LIGHT,
            10,
        )
        .with_experience_reward(100),
        Contract::export_items(
            "DELIVER 100 DIRT BLOCKS",
            500,
            ContractCompany::DeepTekIndustries,
            ItemId::DIRT_BLOCK,
            100,
        )
        .with_experience_reward(120),
        Contract::export_items(
            "DELIVER 2 BATTERIES",
            3_000,
            ContractCompany::VanguardDefence,
            ItemId::BATTERY,
            2,
        )
        .with_experience_reward(250),
        Contract::export_items(
            "DELIVER 1 RED SHAFT BORE",
            6_000,
            ContractCompany::AstraSurveyCorp,
            ItemId::RED_SHAFT_BORE,
            1,
        )
        .with_experience_reward(350),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_contracts_cover_each_company() {
        let contracts = built_in_contracts();
        for company in [
            ContractCompany::DeepTekIndustries,
            ContractCompany::VanguardDefence,
            ContractCompany::AstraSurveyCorp,
        ] {
            assert!(contracts.iter().any(|contract| contract.company == company));
        }
        assert!(
            contracts
                .iter()
                .all(|contract| { !contract.requirement.is_empty() && contract.reward > 0 })
        );
    }

    #[test]
    fn exports_progress_matching_contracts_without_double_counting_items() {
        let mut contracts = vec![
            Contract::export_items(
                "FIRST",
                100,
                ContractCompany::DeepTekIndustries,
                ItemId::STONE_BLOCK,
                5,
            ),
            Contract::export_items(
                "SECOND",
                200,
                ContractCompany::AstraSurveyCorp,
                ItemId::STONE_BLOCK,
                10,
            ),
        ];

        assert_eq!(
            apply_export_to_contracts(&mut contracts, ItemId::STONE_BLOCK, 8),
            ContractExportResult {
                contributed: 8,
                completed_contracts: 1,
            }
        );
        assert_eq!(contracts[0].export_progress().unwrap().exported, 5);
        assert_eq!(contracts[1].export_progress().unwrap().exported, 3);
        assert_eq!(
            apply_export_to_contracts(&mut contracts, ItemId::STONE_BLOCK, 7),
            ContractExportResult {
                contributed: 7,
                completed_contracts: 1,
            }
        );
        assert!(contracts[1].export_progress().unwrap().is_complete());

        assert_eq!(
            apply_export_to_contracts(&mut contracts, ItemId::STONE_BLOCK, 10),
            ContractExportResult::default()
        );
    }

    #[test]
    fn unrelated_exports_do_not_change_contract_progress() {
        let mut contracts = built_in_contracts();
        let before = contracts[0].export_progress().unwrap();
        assert_eq!(
            apply_export_to_contracts(&mut contracts, ItemId::CHEST, 50),
            ContractExportResult::default()
        );
        assert_eq!(contracts[0].export_progress(), Some(before));
    }

    #[test]
    fn board_accepts_at_most_four_contracts() {
        let mut board = ContractBoard::with_built_ins();
        for _ in 0..MAX_ACTIVE_CONTRACTS {
            assert_eq!(board.accept(0), Ok(()));
        }
        assert_eq!(
            board.accept(0),
            Err(AcceptContractError::ActiveLimitReached)
        );
        assert_eq!(board.active().len(), MAX_ACTIVE_CONTRACTS);
    }

    #[test]
    fn completed_reward_can_only_be_claimed_once() {
        let mut board = ContractBoard::new(vec![Contract::export_items(
            "STONE",
            250,
            ContractCompany::DeepTekIndustries,
            ItemId::STONE_BLOCK,
            2,
        )]);
        board.accept(0).unwrap();
        assert_eq!(board.claim_reward(0), Err(ClaimContractError::Incomplete));
        assert_eq!(
            board
                .apply_export(ItemId::STONE_BLOCK, 2)
                .completed_contracts,
            1
        );
        assert_eq!(
            board.claim_reward(0),
            Ok(ContractReward {
                money: 250,
                company: ContractCompany::DeepTekIndustries,
                experience: 100,
            })
        );
        assert_eq!(
            board.claim_reward(0),
            Err(ClaimContractError::InvalidContract)
        );
    }

    #[test]
    fn corporation_experience_levels_and_caps_at_five() {
        let mut progress = CorporationProgress::default();
        let company = ContractCompany::AstraSurveyCorp;

        assert_eq!(progress.level(company), 0);
        assert_eq!(progress.award(company, 249), 249);
        assert_eq!(progress.level(company), 1);
        assert_eq!(progress.next_level_experience(company), Some(250));
        assert_eq!(progress.award(company, 10_000), 1_251);
        assert_eq!(progress.experience(company), 1_500);
        assert_eq!(progress.level(company), MAX_CORPORATION_LEVEL);
        assert_eq!(progress.next_level_experience(company), None);
        assert_eq!(progress.award(company, 100), 0);
    }

    #[test]
    fn mining_contract_counts_only_its_requested_item() {
        let mut board = ContractBoard::new(Vec::new());
        board.add_active(Contract::mine_items(
            "MINE STONE",
            100,
            ContractCompany::DeepTekIndustries,
            ItemId::STONE_BLOCK,
            2,
        ));
        assert_eq!(
            board.apply_mined(ItemId::DIRT_BLOCK, 10),
            ContractProgressResult::default()
        );
        assert_eq!(
            board
                .apply_mined(ItemId::STONE_BLOCK, 2)
                .completed_contracts,
            1
        );
        assert!(board.active()[0].is_complete());
    }

    #[test]
    fn build_and_export_contract_requires_every_placement_and_the_shipment() {
        let mut board = ContractBoard::new(Vec::new());
        board.add_active(Contract::build_and_export(
            "BUILD AND EXPORT",
            100,
            ContractCompany::DeepTekIndustries,
            vec![
                crate::FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
                crate::FurnitureObject::SOLAR_ARRAY,
                crate::FurnitureObject::PYLON,
            ],
            ItemId::STONE_BLOCK,
            2,
        ));
        for object_type in [
            crate::FurnitureObject::ORBITAL_EXPORT_LAUNCHER,
            crate::FurnitureObject::SOLAR_ARRAY,
            crate::FurnitureObject::PYLON,
        ] {
            board.apply_placement(object_type);
        }
        assert!(!board.active()[0].is_complete());
        board.apply_export(ItemId::STONE_BLOCK, 2);
        assert!(board.active()[0].is_complete());
    }
}
