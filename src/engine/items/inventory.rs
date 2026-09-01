use super::{ItemId, ItemRegistry};

pub const HOTBAR_SLOTS: usize = 10;
pub const INVENTORY_COLUMNS: usize = 10;
pub const INVENTORY_ROWS: usize = 4;
pub const INVENTORY_SLOTS: usize = INVENTORY_COLUMNS * INVENTORY_ROWS;
pub const CHEST_COLUMNS: usize = 10;
pub const CHEST_ROWS: usize = 4;
pub const CHEST_SLOTS: usize = CHEST_COLUMNS * CHEST_ROWS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemStack {
    item: ItemId,
    quantity: u16,
}

impl ItemStack {
    pub fn new(item: ItemId, quantity: u16) -> Option<Self> {
        (quantity > 0).then_some(Self { item, quantity })
    }

    pub const fn item(self) -> ItemId {
        self.item
    }

    pub const fn quantity(self) -> u16 {
        self.quantity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotClick {
    Primary,
    Secondary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inventory {
    slots: Vec<Option<ItemStack>>,
    selected_hotbar: usize,
}

/// A reusable fixed-capacity item store for chests and future container
/// furniture. Selection and hotbar behavior remain exclusive to `Inventory`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemContainer {
    slots: Vec<Option<ItemStack>>,
}

impl ItemContainer {
    pub fn new(slot_count: usize) -> Self {
        Self {
            slots: vec![None; slot_count],
        }
    }

    pub fn chest() -> Self {
        Self::new(CHEST_SLOTS)
    }

    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    pub fn slot(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// Removes and returns the first complete stack in slot order. Machine
    /// systems can consume whole batches without depending on storage layout.
    pub fn take_first_stack(&mut self) -> Option<ItemStack> {
        self.slots.iter_mut().find_map(Option::take)
    }

    pub fn take_stack(&mut self, index: usize) -> Option<ItemStack> {
        self.slots.get_mut(index)?.take()
    }

    pub(crate) fn remove_item(&mut self, item: ItemId, quantity: u16) -> u16 {
        remove_item_from_slots(&mut self.slots, item, quantity)
    }

    pub(crate) fn can_add(&self, item: ItemId, quantity: u16, max_stack: u16) -> bool {
        if quantity == 0 {
            return true;
        }
        if max_stack == 0 {
            return false;
        }
        let capacity = self.slots.iter().fold(0_u32, |capacity, slot| {
            capacity
                + match slot {
                    Some(stack) if stack.item == item => {
                        u32::from(max_stack.saturating_sub(stack.quantity))
                    }
                    None => u32::from(max_stack),
                    Some(_) => 0,
                }
        });
        capacity >= u32::from(quantity)
    }

    /// Adds the complete quantity or leaves the container unchanged. This
    /// atomic behavior lets mining systems check storage before removing terrain.
    pub(crate) fn try_add(&mut self, item: ItemId, quantity: u16, max_stack: u16) -> bool {
        if !self.can_add(item, quantity, max_stack) {
            return false;
        }
        let mut remaining = quantity;
        for stack in self.slots.iter_mut().flatten() {
            if stack.item != item || stack.quantity >= max_stack {
                continue;
            }
            let moved = remaining.min(max_stack - stack.quantity);
            stack.quantity += moved;
            remaining -= moved;
            if remaining == 0 {
                return true;
            }
        }
        for slot in &mut self.slots {
            if slot.is_some() {
                continue;
            }
            let moved = remaining.min(max_stack);
            *slot = ItemStack::new(item, moved);
            remaining -= moved;
            if remaining == 0 {
                return true;
            }
        }
        debug_assert_eq!(remaining, 0);
        true
    }

    /// Moves at most one item, choosing the first source slot whose item can
    /// fit in the destination. Both containers remain unchanged if no transfer
    /// is possible.
    pub(crate) fn transfer_one_to(
        &mut self,
        destination: &mut Self,
        registry: &ItemRegistry,
    ) -> bool {
        for source in &mut self.slots {
            let Some(stack) = *source else {
                continue;
            };
            let Some(max_stack) = registry
                .get(stack.item)
                .map(|definition| definition.max_stack)
            else {
                continue;
            };
            if !destination.try_add(stack.item, 1, max_stack) {
                continue;
            }
            *source = ItemStack::new(stack.item, stack.quantity - 1);
            return true;
        }
        false
    }

    pub fn click_slot(
        &mut self,
        index: usize,
        cursor: &mut Option<ItemStack>,
        click: SlotClick,
        registry: &ItemRegistry,
    ) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        match click {
            SlotClick::Primary => primary_click(slot, cursor, registry),
            SlotClick::Secondary => secondary_click(slot, cursor, registry),
        }
    }

    pub(crate) fn set_slot(&mut self, index: usize, stack: Option<ItemStack>) -> bool {
        let Some(slot) = self.slots.get_mut(index) else {
            return false;
        };
        *slot = stack;
        true
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            slots: vec![None; INVENTORY_SLOTS],
            selected_hotbar: 0,
        }
    }

    pub(crate) fn from_saved_slots(
        slots: Vec<Option<ItemStack>>,
        selected_hotbar: usize,
    ) -> Option<Self> {
        (slots.len() == INVENTORY_SLOTS && selected_hotbar < HOTBAR_SLOTS).then_some(Self {
            slots,
            selected_hotbar,
        })
    }

    pub fn starter(registry: &ItemRegistry) -> Self {
        let mut inventory = Self::new();
        assert_eq!(inventory.add(ItemId::PICKAXE, 1, registry), 0);
        assert_eq!(inventory.add(ItemId::ROPE, 100, registry), 0);
        assert_eq!(inventory.add(ItemId::GLOW_STICK, 20, registry), 0);
        inventory
    }

    #[cfg(test)]
    pub(crate) fn test_loadout(registry: &ItemRegistry) -> Self {
        let mut inventory = Self::new();
        for (item, quantity) in [
            (ItemId::DIRT_BLOCK, 200),
            (ItemId::STONE_BLOCK, 200),
            (ItemId::RED_LIGHT, 25),
            (ItemId::PICKAXE, 1),
            (ItemId::GLOW_STICK, 100),
            (ItemId::BOMB, 25),
            (ItemId::HEALING_POTION, 10),
            (ItemId::CHEST, 25),
            (ItemId::LASER_BORE, 10),
            (ItemId::TURRET, 10),
            (ItemId::ORBITAL_EXPORT_LAUNCHER, 5),
            (ItemId::CARGO_CONVEYOR, 200),
            (ItemId::SOLAR_ARRAY, 10),
            (ItemId::PYLON, 50),
            (ItemId::BATTERY, 10),
            (ItemId::ROPE, 200),
            (ItemId::POWERED_CABLE, 300),
            (ItemId::CARGO_LIFT, 5),
            (ItemId::LIFT_STATION, 10),
            (ItemId::POWER_CONNECTOR, 100),
            (ItemId::COMPOSITE_ASSEMBLER, 10),
            (ItemId::RED_SHAFT_BORE, 5),
            (ItemId::PROCUREMENT_TERMINAL, 1),
            (ItemId::LASER_DRILL, 5),
            (ItemId::AMMO_TURRET, 5),
            (ItemId::DIRECTIONAL_SENTRY, 10),
            (ItemId::TURRET_AMMO, 200),
            (ItemId::SPIKES, 50),
            (ItemId::DOOR, 10),
        ] {
            assert_eq!(inventory.add(item, quantity, registry), 0);
        }
        inventory
    }

    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    pub fn slot(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    pub const fn selected_hotbar(&self) -> usize {
        self.selected_hotbar
    }

    pub fn select_hotbar(&mut self, index: usize) {
        self.selected_hotbar = index.min(HOTBAR_SLOTS - 1);
    }

    pub fn cycle_hotbar(&mut self, offset: i32) {
        self.selected_hotbar =
            (self.selected_hotbar as i32 + offset).rem_euclid(HOTBAR_SLOTS as i32) as usize;
    }

    pub fn selected_stack(&self) -> Option<ItemStack> {
        self.slot(self.selected_hotbar)
    }

    pub fn quantity(&self, item: ItemId) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.item == item)
            .map(|stack| u32::from(stack.quantity))
            .sum()
    }

    /// Adds as much as possible and returns the quantity that did not fit.
    pub fn add(&mut self, item: ItemId, quantity: u16, registry: &ItemRegistry) -> u16 {
        let Some(definition) = registry.get(item) else {
            return quantity;
        };
        let mut remaining = quantity;
        for stack in self.slots.iter_mut().flatten() {
            if stack.item != item || stack.quantity >= definition.max_stack {
                continue;
            }
            let moved = remaining.min(definition.max_stack - stack.quantity);
            stack.quantity += moved;
            remaining -= moved;
            if remaining == 0 {
                return 0;
            }
        }
        for slot in &mut self.slots {
            if slot.is_some() {
                continue;
            }
            let moved = remaining.min(definition.max_stack);
            *slot = ItemStack::new(item, moved);
            remaining -= moved;
            if remaining == 0 {
                break;
            }
        }
        remaining
    }

    pub fn remove_from_slot(&mut self, index: usize, quantity: u16) -> u16 {
        let Some(slot) = self.slots.get_mut(index) else {
            return 0;
        };
        let Some(stack) = slot.as_mut() else {
            return 0;
        };
        let removed = quantity.min(stack.quantity);
        stack.quantity -= removed;
        if stack.quantity == 0 {
            *slot = None;
        }
        removed
    }

    pub(crate) fn remove_item(&mut self, item: ItemId, quantity: u16) -> u16 {
        remove_item_from_slots(&mut self.slots, item, quantity)
    }

    pub fn consume_selected(&mut self, quantity: u16) -> u16 {
        self.remove_from_slot(self.selected_hotbar, quantity)
    }

    pub fn click_slot(
        &mut self,
        index: usize,
        cursor: &mut Option<ItemStack>,
        click: SlotClick,
        registry: &ItemRegistry,
    ) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        match click {
            SlotClick::Primary => primary_click(slot, cursor, registry),
            SlotClick::Secondary => secondary_click(slot, cursor, registry),
        }
    }
}

fn remove_item_from_slots(slots: &mut [Option<ItemStack>], item: ItemId, quantity: u16) -> u16 {
    let mut remaining = quantity;
    for slot in slots {
        let Some(stack) = slot.as_mut().filter(|stack| stack.item == item) else {
            continue;
        };
        let removed = remaining.min(stack.quantity);
        stack.quantity -= removed;
        remaining -= removed;
        if stack.quantity == 0 {
            *slot = None;
        }
        if remaining == 0 {
            break;
        }
    }
    quantity - remaining
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

fn primary_click(
    slot: &mut Option<ItemStack>,
    cursor: &mut Option<ItemStack>,
    registry: &ItemRegistry,
) {
    let (Some(slot_stack), Some(cursor_stack)) = (slot.as_mut(), cursor.as_mut()) else {
        std::mem::swap(slot, cursor);
        return;
    };
    if slot_stack.item != cursor_stack.item {
        std::mem::swap(slot, cursor);
        return;
    }
    let Some(definition) = registry.get(slot_stack.item) else {
        return;
    };
    let moved = cursor_stack
        .quantity
        .min(definition.max_stack.saturating_sub(slot_stack.quantity));
    slot_stack.quantity += moved;
    cursor_stack.quantity -= moved;
    if cursor_stack.quantity == 0 {
        *cursor = None;
    }
}

fn secondary_click(
    slot: &mut Option<ItemStack>,
    cursor: &mut Option<ItemStack>,
    registry: &ItemRegistry,
) {
    match (slot.as_mut(), cursor.as_mut()) {
        (Some(slot_stack), None) => {
            let quantity = slot_stack.quantity.div_ceil(2);
            slot_stack.quantity -= quantity;
            *cursor = ItemStack::new(slot_stack.item, quantity);
            if slot_stack.quantity == 0 {
                *slot = None;
            }
        }
        (None, Some(cursor_stack)) => {
            *slot = ItemStack::new(cursor_stack.item, 1);
            cursor_stack.quantity -= 1;
            if cursor_stack.quantity == 0 {
                *cursor = None;
            }
        }
        (Some(slot_stack), Some(cursor_stack)) if slot_stack.item == cursor_stack.item => {
            let Some(definition) = registry.get(slot_stack.item) else {
                return;
            };
            if slot_stack.quantity < definition.max_stack {
                slot_stack.quantity += 1;
                cursor_stack.quantity -= 1;
                if cursor_stack.quantity == 0 {
                    *cursor = None;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_inventory_contains_basic_exploration_equipment() {
        let registry = ItemRegistry::with_built_ins();
        let inventory = Inventory::starter(&registry);
        assert_eq!(inventory.slot(0), ItemStack::new(ItemId::PICKAXE, 1));
        assert_eq!(inventory.slot(1), ItemStack::new(ItemId::ROPE, 100));
        assert_eq!(inventory.slot(2), ItemStack::new(ItemId::GLOW_STICK, 20));
        assert!(inventory.slots()[3..].iter().all(Option::is_none));
    }

    #[test]
    fn add_fills_existing_stacks_before_empty_slots() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        assert_eq!(inventory.add(ItemId::HEALING_POTION, 25, &registry), 0);
        assert_eq!(inventory.add(ItemId::HEALING_POTION, 10, &registry), 0);
        assert_eq!(inventory.slot(0).unwrap().quantity(), 30);
        assert_eq!(inventory.slot(1).unwrap().quantity(), 5);
    }

    #[test]
    fn secondary_click_splits_and_places_single_items() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        inventory.add(ItemId::DIRT_BLOCK, 9, &registry);
        let mut cursor = None;
        inventory.click_slot(0, &mut cursor, SlotClick::Secondary, &registry);
        assert_eq!(inventory.slot(0).unwrap().quantity(), 4);
        assert_eq!(cursor.unwrap().quantity(), 5);
        inventory.click_slot(1, &mut cursor, SlotClick::Secondary, &registry);
        assert_eq!(inventory.slot(1).unwrap().quantity(), 1);
        assert_eq!(cursor.unwrap().quantity(), 4);
    }

    #[test]
    fn tools_do_not_stack_above_their_definition_limit() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        inventory.add(ItemId::PICKAXE, 2, &registry);
        assert_eq!(inventory.slot(0).unwrap().quantity(), 1);
        assert_eq!(inventory.slot(1).unwrap().quantity(), 1);
    }

    #[test]
    fn containers_share_inventory_stack_click_rules() {
        let registry = ItemRegistry::with_built_ins();
        let mut container = ItemContainer::chest();
        let mut cursor = ItemStack::new(ItemId::DIRT_BLOCK, 9);
        container.click_slot(0, &mut cursor, SlotClick::Secondary, &registry);
        assert_eq!(container.slot(0).unwrap().quantity(), 1);
        assert_eq!(cursor.unwrap().quantity(), 8);

        container.click_slot(0, &mut cursor, SlotClick::Primary, &registry);
        assert_eq!(container.slot(0).unwrap().quantity(), 9);
        assert_eq!(cursor, None);
    }

    #[test]
    fn container_try_add_is_atomic_when_capacity_is_insufficient() {
        let mut container = ItemContainer::new(1);
        assert!(container.try_add(ItemId::STONE_BLOCK, 998, 999));
        assert!(!container.try_add(ItemId::STONE_BLOCK, 2, 999));
        assert_eq!(container.slot(0).unwrap().quantity(), 998);
        assert!(container.try_add(ItemId::STONE_BLOCK, 1, 999));
        assert_eq!(container.slot(0).unwrap().quantity(), 999);
    }

    #[test]
    fn container_transfer_moves_exactly_one_item_in_slot_order() {
        let registry = ItemRegistry::with_built_ins();
        let mut source = ItemContainer::new(2);
        let mut destination = ItemContainer::new(1);
        assert!(source.set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 700)));
        assert!(source.set_slot(1, ItemStack::new(ItemId::DIRT_BLOCK, 50)));
        assert!(destination.set_slot(0, ItemStack::new(ItemId::STONE_BLOCK, 900)));

        assert!(source.transfer_one_to(&mut destination, &registry));
        assert_eq!(
            destination.slot(0),
            ItemStack::new(ItemId::STONE_BLOCK, 901)
        );
        assert_eq!(source.slot(0), ItemStack::new(ItemId::STONE_BLOCK, 699));
        assert_eq!(source.slot(1), ItemStack::new(ItemId::DIRT_BLOCK, 50));
    }

    #[test]
    fn taking_first_stack_removes_one_whole_stack_in_slot_order() {
        let mut container = ItemContainer::new(4);
        assert!(container.set_slot(1, ItemStack::new(ItemId::STONE_BLOCK, 12)));
        assert!(container.set_slot(3, ItemStack::new(ItemId::DIRT_BLOCK, 8)));

        assert_eq!(
            container.take_first_stack(),
            ItemStack::new(ItemId::STONE_BLOCK, 12)
        );
        assert_eq!(container.slot(1), None);
        assert_eq!(container.slot(3), ItemStack::new(ItemId::DIRT_BLOCK, 8));
    }
}
