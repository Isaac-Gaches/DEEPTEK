//! One discoverable entry point for data-driven game content.
//!
//! Add content to the exported definition table owned by its subsystem. Runtime
//! state and simulation remain in `engine`; progression and balancing data live
//! under `game`.

pub use crate::contracts::{Contract, ContractId, built_in_contracts};
pub use crate::delivery::{MACHINE_OFFERS, MachineOffer};
pub use crate::entity::{BUILT_IN_LIFEFORMS, LifeformDefinition};
pub use crate::items::{
    CRAFTING_RECIPES, CraftingRecipe, ItemDefinition, ItemId, built_in_item_definitions,
};
pub use crate::specialists::{BUILT_IN_SPECIALISTS, SpecialistDefinition};
pub use crate::terrain::{
    BUILT_IN_BLOCKS, BUILT_IN_DECORATIONS, BUILT_IN_FURNITURE, BlockDefinition,
    DecorationDefinition, FurnitureDefinition,
};
pub use crate::tutorial::{TUTORIAL_MISSIONS, TutorialMissionDefinition};
