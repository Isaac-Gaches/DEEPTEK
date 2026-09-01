#[path = "../../game/content/crafting.rs"]
mod crafting;
mod definition;
mod dropped;
mod inventory;
mod registry;
mod use_item;

pub use crafting::{
    CRAFTING_RECIPES, CraftingError, CraftingIngredient, CraftingRecipe, crafting_recipe,
};
pub(crate) use definition::mined_block_drop;
pub use definition::{
    ConsumableAction, ItemAction, ItemCategory, ItemDefinition, ItemId, ProjectileKind, ToolAction,
};
pub use dropped::{
    DROPPED_ITEM_ICON_FRAMES, DroppedItem, DroppedItemContext, DroppedItemSystem, DroppedItemUpdate,
};
pub use inventory::{
    CHEST_COLUMNS, CHEST_ROWS, CHEST_SLOTS, HOTBAR_SLOTS, INVENTORY_COLUMNS, INVENTORY_ROWS,
    INVENTORY_SLOTS, Inventory, ItemContainer, ItemStack, SlotClick,
};
pub use registry::{ItemRegistry, ItemRegistryError, built_in_item_definitions};
pub use use_item::{
    DEFAULT_ITEM_REACH, ItemTargetStatus, ItemUseResult, selected_item_target_size,
    selected_item_target_status, use_selected_item, use_selected_item_in_background,
};
