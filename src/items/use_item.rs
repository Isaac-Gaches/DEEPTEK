use super::{
    ConsumableAction, Inventory, ItemAction, ItemId, ItemRegistry, ProjectileKind, ToolAction,
};
use crate::{
    Collider, FurnitureObject, Health, Layer, ObjectTypeId, POWERED_CABLE_OBJECT, ROPE_OBJECT,
    TerrainRenderer, TileId, TilePos, Transform, World, furniture_definition,
};
use hecs::{Entity, World as EntityWorld};

/// The prototype does not impose an interaction radius. Callers can still pass
/// a finite value to `use_selected_item` when a tool or game mode needs one.
pub const DEFAULT_ITEM_REACH: f32 = f32::INFINITY;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemUseResult {
    NoItem,
    OutOfReach,
    Blocked,
    Placed(TilePos),
    Removed(TilePos),
    Consumed,
    Thrown(ProjectileKind),
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemTargetStatus {
    NotTargeted,
    Valid(TilePos),
    OutOfReach(TilePos),
    Blocked(TilePos),
}

pub fn selected_item_target_status(
    inventory: &Inventory,
    registry: &ItemRegistry,
    terrain: &World,
    entities: &EntityWorld,
    user: Entity,
    target: Option<TilePos>,
    reach: f32,
) -> ItemTargetStatus {
    let Some(stack) = inventory.selected_stack() else {
        return ItemTargetStatus::NotTargeted;
    };
    let Some(definition) = registry.get(stack.item()) else {
        return ItemTargetStatus::NotTargeted;
    };
    match definition.action {
        ItemAction::PlaceTile { layer, .. } => {
            let Some(target) = target else {
                return ItemTargetStatus::NotTargeted;
            };
            if !target_in_reach(entities, user, target, reach) {
                ItemTargetStatus::OutOfReach(target)
            } else if !terrain.can_place_tile_adjacent(target, layer)
                || layer == Layer::Foreground && tile_overlaps_user(entities, user, target)
            {
                ItemTargetStatus::Blocked(target)
            } else {
                ItemTargetStatus::Valid(target)
            }
        }
        ItemAction::PlaceFurniture { object_type } => {
            let Some(target) = target else {
                return ItemTargetStatus::NotTargeted;
            };
            let anchor = if object_type == FurnitureObject::POWERED_CABLE_ANCHOR {
                match terrain.powered_cable_anchor_placement_target(target) {
                    Ok(anchor) => anchor,
                    Err(_) => return ItemTargetStatus::Blocked(target),
                }
            } else {
                let Some(anchor) = furniture_anchor_from_target(object_type, target) else {
                    return ItemTargetStatus::Blocked(target);
                };
                anchor
            };
            if !target_in_reach(entities, user, target, reach) {
                return ItemTargetStatus::OutOfReach(anchor);
            }
            if terrain.can_place_furniture(object_type, anchor).is_err() {
                ItemTargetStatus::Blocked(anchor)
            } else {
                ItemTargetStatus::Valid(anchor)
            }
        }
        ItemAction::PlaceRope => {
            let Some(target) = target else {
                return ItemTargetStatus::NotTargeted;
            };
            if !target_in_reach(entities, user, target, reach) {
                return ItemTargetStatus::OutOfReach(target);
            }
            match terrain.rope_placement_target(target) {
                Ok(placement) => ItemTargetStatus::Valid(placement),
                Err(_) => ItemTargetStatus::Blocked(target),
            }
        }
        ItemAction::PlacePoweredCable => {
            let Some(target) = target else {
                return ItemTargetStatus::NotTargeted;
            };
            if !target_in_reach(entities, user, target, reach) {
                return ItemTargetStatus::OutOfReach(target);
            }
            match terrain.powered_cable_placement_target(target) {
                Ok(placement) => ItemTargetStatus::Valid(placement),
                Err(_) => ItemTargetStatus::Blocked(target),
            }
        }
        ItemAction::PlaceCargoLift => {
            let Some(target) = target else {
                return ItemTargetStatus::NotTargeted;
            };
            if !target_in_reach(entities, user, target, reach) {
                return ItemTargetStatus::OutOfReach(target);
            }
            let Some(anchor) = furniture_anchor_from_target(FurnitureObject::CARGO_LIFT, target)
            else {
                return ItemTargetStatus::Blocked(target);
            };
            match terrain
                .cargo_lift_placement_target(anchor)
                .or_else(|_| terrain.cargo_lift_placement_target(target))
            {
                Ok(anchor) => ItemTargetStatus::Valid(anchor),
                Err(_) => ItemTargetStatus::Blocked(target),
            }
        }
        ItemAction::Tool(ToolAction::RemoveTile { layer, .. }) => {
            let Some(target) = target else {
                return ItemTargetStatus::NotTargeted;
            };
            let object = (layer == Layer::Foreground)
                .then(|| terrain.object_at(target))
                .flatten();
            if !target_in_reach(entities, user, target, reach) {
                ItemTargetStatus::OutOfReach(target)
            } else if object
                .is_some_and(|object| terrain.container_is_empty(object.id()) == Some(false))
                || object.is_some_and(|object| !terrain.can_remove_object(object.id()))
                || terrain.tile(target.x, target.y, layer).ok() == Some(TileId::EMPTY)
                    && object.is_none()
            {
                ItemTargetStatus::Blocked(target)
            } else {
                ItemTargetStatus::Valid(target)
            }
        }
        _ => ItemTargetStatus::NotTargeted,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn use_selected_item(
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    terrain: &mut World,
    renderer: &mut TerrainRenderer,
    entities: &mut EntityWorld,
    user: Entity,
    target: Option<TilePos>,
    reach: f32,
) -> ItemUseResult {
    let Some(stack) = inventory.selected_stack() else {
        return ItemUseResult::NoItem;
    };
    let Some(definition) = registry.get(stack.item()) else {
        return ItemUseResult::Unsupported;
    };
    let target_status =
        selected_item_target_status(inventory, registry, terrain, entities, user, target, reach);
    match definition.action {
        ItemAction::PlaceTile { layer, tile } => {
            let target = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            if !terrain.can_place_tile_adjacent(target, layer)
                || renderer
                    .set_tile(terrain, target.x, target.y, layer, tile)
                    .is_err()
            {
                return ItemUseResult::Blocked;
            }
            inventory.consume_selected(1);
            ItemUseResult::Placed(target)
        }
        ItemAction::PlaceFurniture { object_type } => {
            let anchor = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            if terrain.place_furniture(object_type, anchor).is_err() {
                return ItemUseResult::Blocked;
            }
            inventory.consume_selected(1);
            ItemUseResult::Placed(anchor)
        }
        ItemAction::PlaceRope => {
            let target = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            let Ok(placement) = terrain.place_or_extend_rope(target) else {
                return ItemUseResult::Blocked;
            };
            inventory.consume_selected(1);
            ItemUseResult::Placed(placement)
        }
        ItemAction::PlacePoweredCable => {
            let target = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            let Ok(placement) = terrain.place_or_extend_powered_cable(target) else {
                return ItemUseResult::Blocked;
            };
            inventory.consume_selected(1);
            ItemUseResult::Placed(placement)
        }
        ItemAction::PlaceCargoLift => {
            let anchor = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            let Ok(lift) = terrain.place_cargo_lift(anchor) else {
                return ItemUseResult::Blocked;
            };
            debug_assert_eq!(
                terrain.object(lift).map(|object| object.anchor()),
                Some(anchor)
            );
            inventory.consume_selected(1);
            ItemUseResult::Placed(anchor)
        }
        ItemAction::Tool(ToolAction::RemoveTile { layer, .. }) => {
            let target = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            if layer == Layer::Foreground
                && let Some(existing) = terrain.object_at(target)
                && !terrain.can_remove_object(existing.id())
            {
                return ItemUseResult::Blocked;
            }
            if layer == Layer::Foreground
                && let Some(object) = terrain.remove_object_at(target)
            {
                let drop = if object.object_type() == ROPE_OBJECT {
                    Some((ItemId::ROPE, object.size()[1]))
                } else if object.object_type() == POWERED_CABLE_OBJECT {
                    Some((ItemId::POWERED_CABLE, object.size()[1]))
                } else {
                    registry
                        .item_for_furniture(object.object_type())
                        .map(|item| (item, 1))
                };
                if let Some((item, quantity)) = drop {
                    let _remaining = inventory.add(item, quantity, registry);
                }
                return ItemUseResult::Removed(object.anchor());
            }
            if renderer
                .set_tile(terrain, target.x, target.y, layer, TileId::EMPTY)
                .is_err()
            {
                return ItemUseResult::Blocked;
            }
            ItemUseResult::Removed(target)
        }
        ItemAction::Consume(ConsumableAction::Heal { amount }) => {
            let Ok(mut health) = entities.get::<&mut Health>(user) else {
                return ItemUseResult::Unsupported;
            };
            if health.heal(amount) == 0 {
                return ItemUseResult::Blocked;
            }
            drop(health);
            inventory.consume_selected(1);
            ItemUseResult::Consumed
        }
        ItemAction::Throw(projectile) => {
            inventory.consume_selected(1);
            ItemUseResult::Thrown(projectile)
        }
        ItemAction::None | ItemAction::Custom(_) => ItemUseResult::Unsupported,
    }
}

pub fn selected_item_target_size(
    inventory: &Inventory,
    registry: &ItemRegistry,
) -> Option<[u16; 2]> {
    let action = registry.get(inventory.selected_stack()?.item())?.action;
    match action {
        ItemAction::PlaceFurniture { object_type } => {
            furniture_definition(object_type).map(|definition| definition.size())
        }
        ItemAction::PlaceRope => Some([1, 1]),
        ItemAction::PlacePoweredCable => Some([1, 1]),
        ItemAction::PlaceCargoLift => Some([2, 2]),
        action if action.has_world_target() => Some([1, 1]),
        _ => None,
    }
}

fn valid_target_or_result(status: ItemTargetStatus) -> Result<TilePos, ItemUseResult> {
    match status {
        ItemTargetStatus::Valid(target) => Ok(target),
        ItemTargetStatus::OutOfReach(_) => Err(ItemUseResult::OutOfReach),
        ItemTargetStatus::Blocked(_) | ItemTargetStatus::NotTargeted => Err(ItemUseResult::Blocked),
    }
}

fn target_in_reach(entities: &EntityWorld, user: Entity, target: TilePos, reach: f32) -> bool {
    let Some(user_position) = entities
        .get::<&Transform>(user)
        .ok()
        .map(|transform| transform.position)
    else {
        return false;
    };
    let target_centre = [target.x as f32, target.y as f32];
    let distance_squared = (target_centre[0] - user_position[0]).powi(2)
        + (target_centre[1] - user_position[1]).powi(2);
    distance_squared <= reach.max(0.0).powi(2)
}

fn tile_overlaps_user(entities: &EntityWorld, user: Entity, tile: TilePos) -> bool {
    let Ok(transform) = entities.get::<&Transform>(user) else {
        return false;
    };
    let Ok(collider) = entities.get::<&Collider>(user) else {
        return false;
    };
    let centre = [
        transform.position[0] + collider.offset[0],
        transform.position[1] + collider.offset[1],
    ];
    let tile_x = tile.x as f32;
    let tile_y = tile.y as f32;
    centre[0] + collider.half_extents[0] > tile_x - 0.5
        && centre[0] - collider.half_extents[0] < tile_x + 0.5
        && centre[1] + collider.half_extents[1] > tile_y - 0.5
        && centre[1] - collider.half_extents[1] < tile_y + 0.5
}

fn furniture_anchor_from_target(object_type: ObjectTypeId, target: TilePos) -> Option<TilePos> {
    let height = furniture_definition(object_type)?.size()[1];
    Some(TilePos::new(
        target.x,
        target.y.checked_sub(u32::from(height).checked_sub(1)?)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // GPU-backed placement is covered by the runtime integration. These unit
    // tests retain the pure interaction rules without constructing a window.
    #[test]
    fn overlap_test_blocks_tiles_inside_the_user_collider() {
        let mut entities = EntityWorld::new();
        let user = entities.spawn((Transform::new([4.0, 4.0]), Collider::new(1.0, 2.0)));
        assert!(tile_overlaps_user(&entities, user, TilePos::new(4, 4)));
        assert!(!tile_overlaps_user(&entities, user, TilePos::new(7, 4)));
    }

    #[test]
    fn placement_status_explains_valid_blocked_and_out_of_reach_targets() {
        const TEST_REACH: f32 = 8.0;
        let registry = ItemRegistry::with_built_ins();
        let inventory = Inventory::starter(&registry);
        let mut terrain = World::empty(20, 20, 0).unwrap();
        for support in [TilePos::new(7, 5), TilePos::new(15, 5)] {
            terrain
                .set_tile(support.x, support.y, Layer::Foreground, TileId::new(2))
                .unwrap();
        }
        let mut entities = EntityWorld::new();
        let user = entities.spawn((Transform::new([4.0, 4.0]), Collider::new(1.0, 2.0)));

        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(7, 4)),
                TEST_REACH,
            ),
            ItemTargetStatus::Valid(TilePos::new(7, 4))
        );
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(12, 4)),
                TEST_REACH,
            ),
            ItemTargetStatus::Blocked(TilePos::new(12, 4))
        );
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(4, 4)),
                TEST_REACH,
            ),
            ItemTargetStatus::Blocked(TilePos::new(4, 4))
        );
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(15, 4)),
                TEST_REACH,
            ),
            ItemTargetStatus::OutOfReach(TilePos::new(15, 4))
        );
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(15, 4)),
                DEFAULT_ITEM_REACH,
            ),
            ItemTargetStatus::Valid(TilePos::new(15, 4))
        );

        terrain
            .set_tile(7, 4, Layer::Foreground, TileId::new(2))
            .unwrap();
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(7, 4)),
                TEST_REACH,
            ),
            ItemTargetStatus::Blocked(TilePos::new(7, 4))
        );
    }

    #[test]
    fn furniture_target_uses_the_bottom_left_cursor_tile_and_full_footprint() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::starter(&registry);
        inventory.select_hotbar(7);
        let mut terrain = World::empty(20, 20, 0).unwrap();
        for x in 7..=8 {
            terrain
                .set_tile(x, 6, Layer::Foreground, TileId::new(2))
                .unwrap();
        }
        let mut entities = EntityWorld::new();
        let user = entities.spawn((Transform::new([2.0, 2.0]), Collider::new(1.0, 2.0)));

        assert_eq!(
            selected_item_target_size(&inventory, &registry),
            Some([2, 2])
        );
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(7, 5)),
                DEFAULT_ITEM_REACH,
            ),
            ItemTargetStatus::Valid(TilePos::new(7, 4))
        );

        terrain
            .set_tile(8, 6, Layer::Foreground, TileId::EMPTY)
            .unwrap();
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(7, 5)),
                DEFAULT_ITEM_REACH,
            ),
            ItemTargetStatus::Blocked(TilePos::new(7, 4))
        );
    }

    #[test]
    fn furniture_placement_is_valid_when_its_footprint_overlaps_the_player() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::starter(&registry);
        inventory.select_hotbar(7);
        let mut terrain = World::empty(12, 12, 0).unwrap();
        for x in 3..=4 {
            terrain
                .set_tile(x, 6, Layer::Foreground, TileId::new(2))
                .unwrap();
        }
        let mut entities = EntityWorld::new();
        let user = entities.spawn((Transform::new([3.5, 4.5]), Collider::new(1.0, 2.0)));

        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(3, 5)),
                DEFAULT_ITEM_REACH,
            ),
            ItemTargetStatus::Valid(TilePos::new(3, 4))
        );
    }

    #[test]
    fn laser_bore_target_previews_its_three_by_three_footprint() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::starter(&registry);
        inventory.select_hotbar(8);
        let mut terrain = World::empty(20, 20, 0).unwrap();
        for x in [7, 9] {
            terrain
                .set_tile(x, 7, Layer::Foreground, TileId::new(2))
                .unwrap();
        }
        let mut entities = EntityWorld::new();
        let user = entities.spawn((Transform::new([2.0, 2.0]), Collider::new(1.0, 2.0)));

        assert_eq!(
            selected_item_target_size(&inventory, &registry),
            Some([3, 3])
        );
        assert_eq!(
            selected_item_target_status(
                &inventory,
                &registry,
                &terrain,
                &entities,
                user,
                Some(TilePos::new(7, 6)),
                DEFAULT_ITEM_REACH,
            ),
            ItemTargetStatus::Valid(TilePos::new(7, 4))
        );
    }
}
