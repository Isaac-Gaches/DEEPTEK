use crate::{FurnitureObject, ItemId, ItemRegistry, ItemStack, ObjectId, PowerSystem, World};
use std::collections::HashMap;
use std::time::Duration;

pub const COMPOSITE_ASSEMBLY_INTERVAL: Duration = Duration::from_secs(1);
const MAX_CATCH_UP_CRAFTS: usize = 8;
const INPUT_SLOTS: [usize; 2] = [0, 1];
const OUTPUT_SLOT: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessingRecipe {
    pub inputs: [(ItemId, u16); 2],
    pub output: (ItemId, u16),
    pub duration: Duration,
}

pub const COMPOSITE_RECIPE: ProcessingRecipe = ProcessingRecipe {
    inputs: [(ItemId::DIRT_BLOCK, 1), (ItemId::STONE_BLOCK, 1)],
    output: (ItemId::HARDENED_COMPOSITE, 1),
    duration: COMPOSITE_ASSEMBLY_INTERVAL,
};

pub const IRON_RECIPE: ProcessingRecipe = ProcessingRecipe {
    inputs: [(ItemId::IRON_ORE, 1), (ItemId::IRON_ORE, 0)],
    output: (ItemId::IRON_INGOT, 1),
    duration: COMPOSITE_ASSEMBLY_INTERVAL,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MachineProcessingUpdate {
    pub machines_checked: usize,
    pub powered_machines: usize,
    pub crafts_completed: usize,
    pub iron_ore_processed: u64,
}

#[derive(Clone, Debug, Default)]
pub struct MachineProcessingSystem {
    progress: HashMap<ObjectId, Duration>,
    machine_ids: Vec<ObjectId>,
}

impl MachineProcessingSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn progress(&self, machine: ObjectId) -> Option<Duration> {
        self.progress.get(&machine).copied()
    }

    pub fn update(
        &mut self,
        world: &mut World,
        registry: &ItemRegistry,
        power: &PowerSystem,
        elapsed: Duration,
    ) -> MachineProcessingUpdate {
        self.update_with_speed(world, registry, power, elapsed, 100)
    }

    pub fn update_with_speed(
        &mut self,
        world: &mut World,
        registry: &ItemRegistry,
        power: &PowerSystem,
        elapsed: Duration,
        speed_percent: u16,
    ) -> MachineProcessingUpdate {
        let elapsed = scale_duration(elapsed, speed_percent);
        self.machine_ids.clear();
        self.machine_ids.extend(
            world
                .objects_of_type(FurnitureObject::COMPOSITE_ASSEMBLER)
                .map(|object| object.id()),
        );
        self.progress
            .retain(|machine, _| world.object(*machine).is_some());
        let mut update = MachineProcessingUpdate::default();
        for machine in self.machine_ids.iter().copied() {
            update.machines_checked += 1;
            if world
                .object(machine)
                .is_none_or(|object| !object.is_active())
                || !power.is_powered(machine)
            {
                continue;
            }
            update.powered_machines += 1;
            let Some(recipe) = available_recipe(world, registry, machine) else {
                self.progress.remove(&machine);
                continue;
            };
            let maximum_progress = recipe.duration.saturating_mul(MAX_CATCH_UP_CRAFTS as u32);
            let progress = self.progress.entry(machine).or_default();
            *progress = progress.saturating_add(elapsed).min(maximum_progress);
            let mut crafts = 0;
            while *progress >= recipe.duration && crafts < MAX_CATCH_UP_CRAFTS {
                if !process_once(world, registry, machine, recipe) {
                    *progress = (*progress).min(recipe.duration);
                    break;
                }
                *progress -= recipe.duration;
                crafts += 1;
            }
            update.crafts_completed += crafts;
            if recipe == IRON_RECIPE {
                update.iron_ore_processed = update.iron_ore_processed.saturating_add(crafts as u64);
            }
        }
        update
    }
}

fn available_recipe(
    world: &World,
    registry: &ItemRegistry,
    machine: ObjectId,
) -> Option<ProcessingRecipe> {
    [IRON_RECIPE, COMPOSITE_RECIPE]
        .into_iter()
        .find(|&recipe| can_process(world, registry, machine, recipe))
}

fn scale_duration(duration: Duration, percent: u16) -> Duration {
    let nanos = duration
        .as_nanos()
        .saturating_mul(u128::from(percent.max(1)))
        / 100;
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn can_process(
    world: &World,
    registry: &ItemRegistry,
    machine: ObjectId,
    recipe: ProcessingRecipe,
) -> bool {
    let Some(container) = world.container(machine) else {
        return false;
    };
    let has_inputs = INPUT_SLOTS
        .into_iter()
        .zip(recipe.inputs)
        .all(|(slot, (item, quantity))| {
            if quantity == 0 {
                return true;
            }
            container
                .slot(slot)
                .is_some_and(|stack| stack.item() == item && stack.quantity() >= quantity)
        });
    if !has_inputs {
        return false;
    }
    let (output, quantity) = recipe.output;
    let Some(maximum) = registry.get(output).map(|definition| definition.max_stack) else {
        return false;
    };
    container
        .slot(OUTPUT_SLOT)
        .map_or(quantity <= maximum, |stack| {
            stack.item() == output && stack.quantity().saturating_add(quantity) <= maximum
        })
}

fn process_once(
    world: &mut World,
    registry: &ItemRegistry,
    machine: ObjectId,
    recipe: ProcessingRecipe,
) -> bool {
    if !can_process(world, registry, machine, recipe) {
        return false;
    }
    let Some(container) = world.container_mut(machine) else {
        return false;
    };
    for (slot, (_, quantity)) in INPUT_SLOTS.into_iter().zip(recipe.inputs) {
        if quantity == 0 {
            continue;
        }
        let stack = container.slot(slot).expect("validated recipe input exists");
        container.set_slot(
            slot,
            ItemStack::new(stack.item(), stack.quantity() - quantity),
        );
    }
    let (output, quantity) = recipe.output;
    let output_quantity = container
        .slot(OUTPUT_SLOT)
        .map_or(quantity, |stack| stack.quantity() + quantity);
    container.set_slot(OUTPUT_SLOT, ItemStack::new(output, output_quantity));
    true
}

pub(crate) fn transfer_one_to_processor(
    world: &mut World,
    source: ObjectId,
    processor: ObjectId,
    registry: &ItemRegistry,
) -> bool {
    if source == processor {
        return false;
    }
    let Some(source_slots) = world
        .container(source)
        .map(|container| container.slots().to_vec())
    else {
        return false;
    };
    for (source_slot, stack) in source_slots.into_iter().enumerate() {
        let Some(stack) = stack else {
            continue;
        };
        let Some(input_slot) = processor_input_slot(stack.item()) else {
            continue;
        };
        let Some(maximum) = registry
            .get(stack.item())
            .map(|definition| definition.max_stack)
        else {
            continue;
        };
        if world
            .container(processor)
            .and_then(|container| container.slot(input_slot))
            .is_some_and(|input| input.item() != stack.item() || input.quantity() >= maximum)
        {
            continue;
        }
        let input_quantity = world
            .container(processor)
            .and_then(|container| container.slot(input_slot))
            .map_or(1, |input| input.quantity() + 1);
        world
            .container_mut(processor)
            .expect("processor container exists")
            .set_slot(input_slot, ItemStack::new(stack.item(), input_quantity));
        world
            .container_mut(source)
            .expect("source container still exists")
            .set_slot(
                source_slot,
                ItemStack::new(stack.item(), stack.quantity() - 1),
            );
        return true;
    }
    false
}

pub(crate) fn transfer_one_from_processor(
    world: &mut World,
    processor: ObjectId,
    destination: ObjectId,
    registry: &ItemRegistry,
) -> bool {
    if processor == destination {
        return false;
    }
    if world
        .object(processor)
        .is_none_or(|object| object.object_type() != FurnitureObject::COMPOSITE_ASSEMBLER)
    {
        return false;
    }
    let Some(stack) = world
        .container(processor)
        .and_then(|container| container.slot(OUTPUT_SLOT))
    else {
        return false;
    };
    // Conveyor extraction is output-only even for malformed legacy or debug
    // state. Items accepted by an input slot can never leave a processor.
    if processor_input_slot(stack.item()).is_some() {
        return false;
    }
    let Some(maximum) = registry
        .get(stack.item())
        .map(|definition| definition.max_stack)
    else {
        return false;
    };
    if !world
        .container(destination)
        .is_some_and(|container| container.can_add(stack.item(), 1, maximum))
    {
        return false;
    }
    world
        .container_mut(destination)
        .expect("destination container exists")
        .try_add(stack.item(), 1, maximum);
    world
        .container_mut(processor)
        .expect("processor container still exists")
        .set_slot(
            OUTPUT_SLOT,
            ItemStack::new(stack.item(), stack.quantity() - 1),
        );
    true
}

fn processor_input_slot(item: ItemId) -> Option<usize> {
    match item {
        ItemId::DIRT_BLOCK | ItemId::IRON_ORE => Some(0),
        ItemId::STONE_BLOCK => Some(1),
        _ => None,
    }
}

/// Returns whether a manually inserted item belongs in the selected processor
/// input. The output slot is deliberately excluded.
pub fn processor_accepts_manual_input(slot: usize, item: ItemId) -> bool {
    processor_input_slot(item) == Some(slot)
}

#[cfg(test)]
mod tests;
