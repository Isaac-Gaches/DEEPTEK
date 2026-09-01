use super::{Inventory, ItemId, ItemRegistry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CraftingIngredient {
    pub item: ItemId,
    pub quantity: u16,
}

impl CraftingIngredient {
    pub const fn new(item: ItemId, quantity: u16) -> Self {
        Self { item, quantity }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CraftingRecipe {
    pub output: CraftingIngredient,
    pub ingredients: &'static [CraftingIngredient],
}

impl CraftingRecipe {
    pub fn can_craft(
        self,
        inventory: &Inventory,
        registry: &ItemRegistry,
    ) -> Result<(), CraftingError> {
        let mut probe = inventory.clone();
        apply_recipe(self, &mut probe, registry)
    }

    pub fn craft(
        self,
        inventory: &mut Inventory,
        registry: &ItemRegistry,
    ) -> Result<(), CraftingError> {
        let mut result = inventory.clone();
        apply_recipe(self, &mut result, registry)?;
        *inventory = result;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CraftingError {
    UnknownItem(ItemId),
    MissingIngredients,
    InventoryFull,
}

const ROPE_INGREDIENTS: &[CraftingIngredient] = &[CraftingIngredient::new(ItemId::DIRT_BLOCK, 2)];
const GLOW_STICK_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::DIRT_BLOCK, 1),
    CraftingIngredient::new(ItemId::STONE_BLOCK, 1),
];
const BOMB_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::STONE_BLOCK, 6),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 1),
];
const POTION_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::DIRT_BLOCK, 4),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 1),
];
const CHEST_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::DIRT_BLOCK, 8),
    CraftingIngredient::new(ItemId::STONE_BLOCK, 4),
];
const CONVEYOR_INGREDIENTS: &[CraftingIngredient] =
    &[CraftingIngredient::new(ItemId::STONE_BLOCK, 2)];
const POWERED_CABLE_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::STONE_BLOCK, 2),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 1),
];
const POWER_CONNECTOR_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::STONE_BLOCK, 3),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 1),
];
const PICKAXE_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::STONE_BLOCK, 8),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 2),
];
const TERMINAL_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::STONE_BLOCK, 12),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 4),
];
const TURRET_AMMO_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::STONE_BLOCK, 2),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 1),
];
const SPIKES_INGREDIENTS: &[CraftingIngredient] = &[
    CraftingIngredient::new(ItemId::STONE_BLOCK, 2),
    CraftingIngredient::new(ItemId::HARDENED_COMPOSITE, 1),
];

/// Hand-crafted outputs deliberately exclude mined blocks, procured machines,
/// and the assembler's processed material. This gives otherwise unavailable
/// utility items a renewable acquisition path without bypassing those systems.
pub const CRAFTING_RECIPES: &[CraftingRecipe] = &[
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::ROPE, 5),
        ingredients: ROPE_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::GLOW_STICK, 4),
        ingredients: GLOW_STICK_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::BOMB, 2),
        ingredients: BOMB_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::HEALING_POTION, 1),
        ingredients: POTION_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::CHEST, 1),
        ingredients: CHEST_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::CARGO_CONVEYOR, 4),
        ingredients: CONVEYOR_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::POWERED_CABLE, 4),
        ingredients: POWERED_CABLE_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::POWER_CONNECTOR, 1),
        ingredients: POWER_CONNECTOR_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::PICKAXE, 1),
        ingredients: PICKAXE_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::PROCUREMENT_TERMINAL, 1),
        ingredients: TERMINAL_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::TURRET_AMMO, 12),
        ingredients: TURRET_AMMO_INGREDIENTS,
    },
    CraftingRecipe {
        output: CraftingIngredient::new(ItemId::SPIKES, 4),
        ingredients: SPIKES_INGREDIENTS,
    },
];

pub fn crafting_recipe(output: ItemId) -> Option<&'static CraftingRecipe> {
    CRAFTING_RECIPES
        .iter()
        .find(|recipe| recipe.output.item == output)
}

fn apply_recipe(
    recipe: CraftingRecipe,
    inventory: &mut Inventory,
    registry: &ItemRegistry,
) -> Result<(), CraftingError> {
    if registry.get(recipe.output.item).is_none() {
        return Err(CraftingError::UnknownItem(recipe.output.item));
    }
    for ingredient in recipe.ingredients {
        if registry.get(ingredient.item).is_none() {
            return Err(CraftingError::UnknownItem(ingredient.item));
        }
        if inventory.quantity(ingredient.item) < u32::from(ingredient.quantity) {
            return Err(CraftingError::MissingIngredients);
        }
    }
    for ingredient in recipe.ingredients {
        let removed = inventory.remove_item(ingredient.item, ingredient.quantity);
        debug_assert_eq!(removed, ingredient.quantity);
    }
    if inventory.add(recipe.output.item, recipe.output.quantity, registry) != 0 {
        return Err(CraftingError::InventoryFull);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MACHINE_OFFERS;

    #[test]
    fn craft_is_atomic_and_consumes_ingredients_across_stacks() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        inventory.add(ItemId::STONE_BLOCK, 6, &registry);
        inventory.add(ItemId::HARDENED_COMPOSITE, 1, &registry);
        let recipe = *crafting_recipe(ItemId::BOMB).unwrap();

        assert_eq!(recipe.craft(&mut inventory, &registry), Ok(()));
        assert_eq!(inventory.quantity(ItemId::STONE_BLOCK), 0);
        assert_eq!(inventory.quantity(ItemId::HARDENED_COMPOSITE), 0);
        assert_eq!(inventory.quantity(ItemId::BOMB), 2);
        let snapshot = inventory.clone();
        assert_eq!(
            recipe.craft(&mut inventory, &registry),
            Err(CraftingError::MissingIngredients)
        );
        assert_eq!(inventory, snapshot);
    }

    #[test]
    fn hand_crafted_outputs_are_not_mined_procured_or_machine_processed() {
        let processed = crate::COMPOSITE_RECIPE.output.0;
        for recipe in CRAFTING_RECIPES {
            assert_ne!(recipe.output.item, ItemId::DIRT_BLOCK);
            assert_ne!(recipe.output.item, ItemId::STONE_BLOCK);
            assert_ne!(recipe.output.item, ItemId::RED_LIGHT);
            assert_ne!(recipe.output.item, processed);
            assert!(
                !MACHINE_OFFERS
                    .iter()
                    .any(|offer| offer.item == recipe.output.item)
            );
        }
    }

    #[test]
    fn full_inventory_does_not_consume_recipe_ingredients() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        assert_eq!(inventory.add(ItemId::STONE_BLOCK, 7, &registry), 0);
        assert_eq!(inventory.add(ItemId::HARDENED_COMPOSITE, 2, &registry), 0);
        assert_eq!(inventory.add(ItemId::PICKAXE, 38, &registry), 0);
        let snapshot = inventory.clone();
        let recipe = *crafting_recipe(ItemId::BOMB).unwrap();

        assert_eq!(
            recipe.craft(&mut inventory, &registry),
            Err(CraftingError::InventoryFull)
        );
        assert_eq!(inventory, snapshot);
    }

    #[test]
    fn ballistic_rounds_are_crafted_in_useful_batches() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        assert_eq!(inventory.add(ItemId::STONE_BLOCK, 2, &registry), 0);
        assert_eq!(inventory.add(ItemId::HARDENED_COMPOSITE, 1, &registry), 0);

        assert_eq!(
            crafting_recipe(ItemId::TURRET_AMMO)
                .unwrap()
                .craft(&mut inventory, &registry),
            Ok(())
        );
        assert_eq!(inventory.quantity(ItemId::TURRET_AMMO), 12);
    }

    #[test]
    fn spikes_are_crafted_in_placeable_rows() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        assert_eq!(inventory.add(ItemId::STONE_BLOCK, 2, &registry), 0);
        assert_eq!(inventory.add(ItemId::HARDENED_COMPOSITE, 1, &registry), 0);

        assert_eq!(
            crafting_recipe(ItemId::SPIKES)
                .unwrap()
                .craft(&mut inventory, &registry),
            Ok(())
        );
        assert_eq!(inventory.quantity(ItemId::SPIKES), 4);
    }
}
