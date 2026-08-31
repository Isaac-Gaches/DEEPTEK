use super::{Inventory, ItemCategory, ItemId, ItemRegistry, ItemStack, mined_block_drop};
use crate::{
    BrokenTile, Collider, Layer, POWERED_CABLE_OBJECT, ROPE_OBJECT, RemovedObject, Sprite,
    TerrainRenderer, TilePos, Transform, World,
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World as EntityWorld};

pub const DROPPED_ITEM_ICON_FRAMES: u32 = 14;
const PICKUP_DELAY_SECONDS: f32 = 0.35;
const PICKUP_RANGE: [f32; 2] = [1.5, 1.75];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DroppedItem {
    stack: ItemStack,
    pickup_delay: f32,
}

impl DroppedItem {
    pub const fn stack(self) -> ItemStack {
        self.stack
    }

    pub const fn can_pick_up(self) -> bool {
        self.pickup_delay <= 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DroppedItemUpdate {
    pub collected_stacks: usize,
    pub collected_items: u32,
}

pub struct DroppedItemContext<'a> {
    pub system: &'a mut DroppedItemSystem,
    pub material: Handle<Material>,
    pub registry: &'a ItemRegistry,
}

impl<'a> DroppedItemContext<'a> {
    pub const fn new(
        system: &'a mut DroppedItemSystem,
        material: Handle<Material>,
        registry: &'a ItemRegistry,
    ) -> Self {
        Self {
            system,
            material,
            registry,
        }
    }
}

/// Owns deterministic item scattering and pickup. One ECS entity represents a
/// complete stack, keeping large container breaks bounded by slot count rather
/// than item quantity.
#[derive(Debug, Default)]
pub struct DroppedItemSystem {
    spawn_sequence: u32,
}

impl DroppedItemSystem {
    #[allow(clippy::too_many_arguments)]
    pub fn break_target(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        registry: &ItemRegistry,
        terrain: &mut World,
        renderer: &mut TerrainRenderer,
        target: TilePos,
        layer: Layer,
    ) -> Option<TilePos> {
        let result_position = (layer == Layer::Foreground)
            .then(|| terrain.object_at(target))
            .flatten()
            .map_or(target, |object| object.anchor());
        self.damage_target(
            entities,
            material,
            registry,
            terrain,
            renderer,
            target,
            layer,
            u16::MAX,
        )?
        .then_some(result_position)
    }

    #[allow(clippy::too_many_arguments)]
    /// Damages a tile or removes a furniture object. Returns whether the target
    /// was fully destroyed, while successful partial block hits return `false`.
    pub fn damage_target(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        registry: &ItemRegistry,
        terrain: &mut World,
        renderer: &mut TerrainRenderer,
        target: TilePos,
        layer: Layer,
        damage: u16,
    ) -> Option<bool> {
        if layer == Layer::Foreground
            && let Some(object) = terrain.object_at(target)
        {
            if !terrain.can_remove_object_with_dependents(object.id()) {
                return None;
            }
            let removed = terrain.remove_object_at_with_dependents(target);
            if removed.is_empty() {
                return None;
            }
            for object in removed {
                self.spawn_removed_object(entities, material, registry, object);
            }
            return Some(true);
        }

        let result = terrain.damage_block(target, layer, damage).ok()?;
        if result.applied == 0 {
            return None;
        }
        if let Some(broken) = result.broken {
            renderer.mark_tile_dirty(target.x, target.y, layer);
            self.spawn_broken_tile(entities, material, registry, broken);
            Some(true)
        } else {
            Some(false)
        }
    }

    pub fn spawn_broken_tile(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        registry: &ItemRegistry,
        broken: BrokenTile,
    ) {
        if let Some((item, _)) = mined_block_drop(broken.tile, broken.layer)
            && let Some(stack) = ItemStack::new(item, 1)
        {
            self.spawn_stack(
                entities,
                material,
                registry,
                stack,
                [broken.position.x as f32, broken.position.y as f32],
            );
        }
        for removed in broken.unsupported_objects {
            self.spawn_removed_object(entities, material, registry, removed);
        }
    }

    pub fn spawn_stack(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        registry: &ItemRegistry,
        stack: ItemStack,
        position: [f32; 2],
    ) -> Option<Entity> {
        let definition = registry.get(stack.item())?;
        let frame = if definition.icon < DROPPED_ITEM_ICON_FRAMES {
            definition.icon
        } else {
            match definition.category {
                ItemCategory::Furniture => 13,
                ItemCategory::Block => 5,
                ItemCategory::Tool => 1,
                ItemCategory::Consumable => 3,
                ItemCategory::Material | ItemCategory::Custom => 4,
            }
        };
        let spread = (self.spawn_sequence % 7) as f32 - 3.0;
        self.spawn_sequence = self.spawn_sequence.wrapping_add(1);
        Some(
            entities.spawn((
                DroppedItem {
                    stack,
                    pickup_delay: PICKUP_DELAY_SECONDS,
                },
                Transform::new(position).with_scale([0.65; 2]),
                Collider::new(0.55, 0.55)
                    .with_velocity([spread * 0.65, -3.5 - spread.abs() * 0.15])
                    .with_material(0.08, 0.82)
                    .with_drag(0.06, 8.0),
                Sprite::new(material).with_frame(frame).with_depth(0.075),
            )),
        )
    }

    pub fn update(
        &self,
        entities: &mut EntityWorld,
        inventory: &mut Inventory,
        registry: &ItemRegistry,
        player_position: [f32; 2],
        elapsed: f32,
    ) -> DroppedItemUpdate {
        let mut update = DroppedItemUpdate::default();
        let mut collected = Vec::new();
        for (entity, (drop, transform)) in entities.query::<(&mut DroppedItem, &Transform)>().iter()
        {
            drop.pickup_delay = (drop.pickup_delay - elapsed.max(0.0)).max(0.0);
            let difference = [
                (transform.position[0] - player_position[0]).abs(),
                (transform.position[1] - player_position[1]).abs(),
            ];
            if !drop.can_pick_up()
                || difference[0] > PICKUP_RANGE[0]
                || difference[1] > PICKUP_RANGE[1]
            {
                continue;
            }
            let original = drop.stack.quantity();
            let remaining = inventory.add(drop.stack.item(), original, registry);
            update.collected_items += u32::from(original - remaining);
            if remaining == 0 {
                collected.push(entity);
                update.collected_stacks += 1;
            } else {
                drop.stack = ItemStack::new(drop.stack.item(), remaining)
                    .expect("a partially collected stack remains non-empty");
            }
        }
        for entity in collected {
            let _ = entities.despawn(entity);
        }
        update
    }

    pub fn spawn_removed_object(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        registry: &ItemRegistry,
        removed: RemovedObject,
    ) {
        let (object, contents) = removed.into_parts();
        let [width, height] = object.size();
        let position = [
            object.anchor().x as f32 + (f32::from(width) - 1.0) * 0.5,
            object.anchor().y as f32 + (f32::from(height) - 1.0) * 0.5,
        ];
        let base_drop = if object.object_type() == ROPE_OBJECT {
            ItemStack::new(ItemId::ROPE, height)
        } else if object.object_type() == POWERED_CABLE_OBJECT {
            ItemStack::new(ItemId::POWERED_CABLE, height)
        } else {
            registry
                .item_for_furniture(object.object_type())
                .and_then(|item| ItemStack::new(item, 1))
        };
        if let Some(stack) = base_drop {
            self.spawn_stack(entities, material, registry, stack, position);
        }
        for stack in contents {
            self.spawn_stack(entities, material, registry, stack, position);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::marker::PhantomData;

    fn material() -> Handle<Material> {
        Handle {
            index: 0,
            generation: 0,
            _marker: PhantomData,
        }
    }

    #[test]
    fn pickup_preserves_the_remainder_when_inventory_only_partially_fits() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        for _ in 0..39 {
            assert_eq!(inventory.add(ItemId::STONE_BLOCK, 999, &registry), 0);
        }
        assert_eq!(inventory.add(ItemId::STONE_BLOCK, 998, &registry), 0);
        let mut entities = EntityWorld::new();
        let mut system = DroppedItemSystem::default();
        system.spawn_stack(
            &mut entities,
            material(),
            &registry,
            ItemStack::new(ItemId::STONE_BLOCK, 5).unwrap(),
            [4.0, 4.0],
        );

        let update = system.update(&mut entities, &mut inventory, &registry, [4.0, 4.0], 1.0);

        assert_eq!(update.collected_items, 1);
        assert_eq!(update.collected_stacks, 0);
        assert_eq!(
            entities
                .query::<&DroppedItem>()
                .iter()
                .next()
                .unwrap()
                .1
                .stack(),
            ItemStack::new(ItemId::STONE_BLOCK, 4).unwrap()
        );
    }

    #[test]
    fn a_broken_block_and_unsupported_furniture_emit_their_items_and_contents() {
        let registry = ItemRegistry::with_built_ins();
        let mut terrain = World::empty(12, 12, 0).unwrap();
        for x in 2..=3 {
            terrain
                .set_tile(x, 6, Layer::Foreground, crate::ForegroundTile::STONE)
                .unwrap();
        }
        let chest = terrain
            .place_furniture(crate::FurnitureObject::CHEST, TilePos::new(2, 4))
            .unwrap();
        assert!(
            terrain
                .container_mut(chest)
                .unwrap()
                .set_slot(0, ItemStack::new(ItemId::DIRT_BLOCK, 12))
        );
        let broken = terrain
            .break_tile(TilePos::new(2, 6), Layer::Foreground)
            .unwrap()
            .unwrap();
        let mut entities = EntityWorld::new();
        let mut system = DroppedItemSystem::default();

        system.spawn_broken_tile(&mut entities, material(), &registry, broken);

        let mut stacks: Vec<_> = entities
            .query::<&DroppedItem>()
            .iter()
            .map(|(_, drop)| drop.stack())
            .collect();
        stacks.sort_unstable_by_key(|stack| (stack.item().raw(), stack.quantity()));
        assert_eq!(
            stacks,
            vec![
                ItemStack::new(ItemId::DIRT_BLOCK, 12).unwrap(),
                ItemStack::new(ItemId::STONE_BLOCK, 1).unwrap(),
                ItemStack::new(ItemId::CHEST, 1).unwrap(),
            ]
        );
    }

    #[test]
    fn broken_background_walls_drop_the_matching_block_item() {
        let registry = ItemRegistry::with_built_ins();
        let mut entities = EntityWorld::new();
        let mut system = DroppedItemSystem::default();
        system.spawn_broken_tile(
            &mut entities,
            material(),
            &registry,
            BrokenTile {
                position: TilePos::new(3, 4),
                layer: Layer::Background,
                tile: crate::BackgroundTile::STONE_WALL,
                unsupported_objects: Vec::new(),
            },
        );

        let mut query = entities.query::<&DroppedItem>();
        let (_, dropped) = query.iter().next().unwrap();
        assert_eq!(dropped.stack().item(), ItemId::STONE_BLOCK);
    }

    #[test]
    fn breaking_a_structural_sentry_drops_supported_furniture_too() {
        let registry = ItemRegistry::with_built_ins();
        let mut terrain = World::empty(12, 12, 0).unwrap();
        terrain
            .set_tile(5, 6, Layer::Background, crate::BackgroundTile::STONE_WALL)
            .unwrap();
        terrain
            .place_furniture(
                crate::FurnitureObject::DIRECTIONAL_SENTRY,
                TilePos::new(5, 6),
            )
            .unwrap();
        terrain
            .place_furniture(crate::FurnitureObject::SPIKES, TilePos::new(5, 5))
            .unwrap();
        let mut entities = EntityWorld::new();
        let mut system = DroppedItemSystem::default();

        let removed = terrain.remove_object_at_with_dependents(TilePos::new(5, 6));
        assert_eq!(removed.len(), 2);
        for object in removed {
            system.spawn_removed_object(&mut entities, material(), &registry, object);
        }

        let mut items: Vec<_> = entities
            .query::<&DroppedItem>()
            .iter()
            .map(|(_, drop)| drop.stack().item())
            .collect();
        items.sort_unstable_by_key(|item| item.raw());
        assert_eq!(items, vec![ItemId::DIRECTIONAL_SENTRY, ItemId::SPIKES]);
    }
}
