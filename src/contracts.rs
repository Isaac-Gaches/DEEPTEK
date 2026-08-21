use crate::ItemId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContractCompany {
    DeepTekIndustries,
    VanguardDefence,
    AstraSurveyCorp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContractObjective {
    ExportItems {
        item: ItemId,
        exported: u64,
        required: u64,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContractExportResult {
    /// Number of exported items assigned to matching unfinished contracts.
    pub contributed: u64,
    /// Sum of rewards for contracts completed by this export.
    pub reward: u64,
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
    pub requirement: String,
    pub reward: u64,
    pub company: ContractCompany,
    objective: Option<ContractObjective>,
}

impl Contract {
    pub fn new(requirement: impl Into<String>, reward: u64, company: ContractCompany) -> Self {
        Self {
            requirement: requirement.into(),
            reward,
            company,
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
            requirement: requirement.into(),
            reward,
            company,
            objective: Some(ContractObjective::ExportItems {
                item,
                exported: 0,
                required,
            }),
        }
    }

    pub const fn export_progress(&self) -> Option<ExportContractProgress> {
        match self.objective {
            Some(ContractObjective::ExportItems {
                item,
                exported,
                required,
            }) => Some(ExportContractProgress {
                item,
                exported,
                required,
            }),
            None => None,
        }
    }

    fn contribute_export(&mut self, item: ItemId, available: u64) -> ContractExportResult {
        let Some(ContractObjective::ExportItems {
            item: required_item,
            exported,
            required,
        }) = self.objective.as_mut()
        else {
            return ContractExportResult::default();
        };
        if *required_item != item || *exported >= *required || available == 0 {
            return ContractExportResult::default();
        }

        let contributed = available.min(*required - *exported);
        *exported += contributed;
        ContractExportResult {
            contributed,
            reward: if *exported == *required {
                self.reward
            } else {
                0
            },
        }
    }
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
        result.reward = result.reward.saturating_add(contribution.reward);
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
        ),
        Contract::new(
            "ELIMINATE 5 SURFACE WALKERS",
            1_800,
            ContractCompany::VanguardDefence,
        ),
        Contract::new(
            "SURVEY THE EASTERN CAVERN",
            1_500,
            ContractCompany::AstraSurveyCorp,
        ),
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
                reward: 100,
            }
        );
        assert_eq!(contracts[0].export_progress().unwrap().exported, 5);
        assert_eq!(contracts[1].export_progress().unwrap().exported, 3);
        assert_eq!(
            apply_export_to_contracts(&mut contracts, ItemId::STONE_BLOCK, 7),
            ContractExportResult {
                contributed: 7,
                reward: 200,
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
            apply_export_to_contracts(&mut contracts, ItemId::DIRT_BLOCK, 50),
            ContractExportResult::default()
        );
        assert_eq!(contracts[0].export_progress(), Some(before));
    }
}
