use super::{
    ConsumableAction, DroppedItemSystem, Inventory, ItemAction, ItemRegistry, ProjectileKind,
    ToolAction,
};
use crate::{
    Collider, FurnitureFacing, FurnitureObject, Health, Layer, ObjectTypeId, TerrainRenderer,
    TileId, TilePos, Transform, World, background_tile_for, furniture_definition,
};
use easy_gpu::{assets::Material, assets_manager::Handle};
use hecs::{Entity, World as EntityWorld};

/// Shared player reach for mining, construction, and world interaction.
pub const DEFAULT_ITEM_REACH: f32 = 5.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemUseResult {
    NoItem,
    OutOfReach,
    Blocked,
    Placed(TilePos),
    Damaged(TilePos),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemUseMode {
    Primary,
    Background,
}

impl ItemUseMode {
    fn resolve(self, action: ItemAction) -> Option<ItemAction> {
        match (self, action) {
            (Self::Primary, action) => Some(action),
            (Self::Background, ItemAction::PlaceTile { tile, .. }) => background_tile_for(tile)
                .map(|tile| ItemAction::PlaceTile {
                    layer: Layer::Background,
                    tile,
                }),
            (Self::Background, ItemAction::Tool(ToolAction::RemoveTile { power, .. })) => {
                Some(ItemAction::Tool(ToolAction::RemoveTile {
                    layer: Layer::Background,
                    power,
                }))
            }
            (Self::Background, _) => None,
        }
    }
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
    selected_item_target_status_for_action(
        definition.action,
        terrain,
        entities,
        user,
        target,
        reach,
    )
}

fn selected_item_target_status_for_action(
    action: ItemAction,
    terrain: &World,
    entities: &EntityWorld,
    user: Entity,
    target: Option<TilePos>,
    reach: f32,
) -> ItemTargetStatus {
    match action {
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
        ItemAction::PlaceCargoLift { object_type } => {
            let Some(target) = target else {
                return ItemTargetStatus::NotTargeted;
            };
            if !target_in_reach(entities, user, target, reach) {
                return ItemTargetStatus::OutOfReach(target);
            }
            let Some(anchor) = furniture_anchor_from_target(object_type, target) else {
                return ItemTargetStatus::Blocked(target);
            };
            match terrain
                .cargo_lift_placement_target_for(object_type, anchor)
                .or_else(|_| terrain.cargo_lift_placement_target_for(object_type, target))
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
                .is_some_and(|object| !terrain.can_remove_object_with_dependents(object.id()))
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
    dropped_items: &mut DroppedItemSystem,
    dropped_item_material: Handle<Material>,
    entities: &mut EntityWorld,
    user: Entity,
    target: Option<TilePos>,
    reach: f32,
) -> ItemUseResult {
    use_selected_item_with_mode(
        inventory,
        registry,
        terrain,
        renderer,
        dropped_items,
        dropped_item_material,
        entities,
        user,
        target,
        reach,
        ItemUseMode::Primary,
    )
}

/// Uses the selected placeable block or tool against the background layer.
/// Other item actions remain reserved for their normal primary interaction.
#[allow(clippy::too_many_arguments)]
pub fn use_selected_item_in_background(
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    terrain: &mut World,
    renderer: &mut TerrainRenderer,
    dropped_items: &mut DroppedItemSystem,
    dropped_item_material: Handle<Material>,
    entities: &mut EntityWorld,
    user: Entity,
    target: Option<TilePos>,
    reach: f32,
) -> ItemUseResult {
    use_selected_item_with_mode(
        inventory,
        registry,
        terrain,
        renderer,
        dropped_items,
        dropped_item_material,
        entities,
        user,
        target,
        reach,
        ItemUseMode::Background,
    )
}

#[allow(clippy::too_many_arguments)]
fn use_selected_item_with_mode(
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    terrain: &mut World,
    renderer: &mut TerrainRenderer,
    dropped_items: &mut DroppedItemSystem,
    dropped_item_material: Handle<Material>,
    entities: &mut EntityWorld,
    user: Entity,
    target: Option<TilePos>,
    reach: f32,
    mode: ItemUseMode,
) -> ItemUseResult {
    let Some(stack) = inventory.selected_stack() else {
        return ItemUseResult::NoItem;
    };
    let Some(definition) = registry.get(stack.item()) else {
        return ItemUseResult::Unsupported;
    };
    let Some(action) = mode.resolve(definition.action) else {
        return ItemUseResult::Unsupported;
    };
    let target_status =
        selected_item_target_status_for_action(action, terrain, entities, user, target, reach);
    match action {
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
            let facing = placement_facing(entities, user, object_type, anchor);
            if terrain
                .place_furniture_facing(object_type, anchor, facing)
                .is_err()
            {
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
        ItemAction::PlaceCargoLift { object_type } => {
            let anchor = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            let Ok(lift) = terrain.place_cargo_lift_type(object_type, anchor) else {
                return ItemUseResult::Blocked;
            };
            debug_assert_eq!(
                terrain.object(lift).map(|object| object.anchor()),
                Some(anchor)
            );
            inventory.consume_selected(1);
            ItemUseResult::Placed(anchor)
        }
        ItemAction::Tool(ToolAction::RemoveTile { layer, power }) => {
            let target = match valid_target_or_result(target_status) {
                Ok(target) => target,
                Err(result) => return result,
            };
            dropped_items
                .damage_target(
                    entities,
                    dropped_item_material,
                    registry,
                    terrain,
                    renderer,
                    target,
                    layer,
                    power,
                )
                .map_or(ItemUseResult::Blocked, |broken| {
                    if broken {
                        ItemUseResult::Removed(target)
                    } else {
                        ItemUseResult::Damaged(target)
                    }
                })
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

fn placement_facing(
    entities: &EntityWorld,
    user: Entity,
    object_type: ObjectTypeId,
    anchor: TilePos,
) -> FurnitureFacing {
    let Some(definition) =
        furniture_definition(object_type).filter(|value| value.supports_facing())
    else {
        return FurnitureFacing::Right;
    };
    let user_x = entities
        .get::<&Transform>(user)
        .ok()
        .map_or(anchor.x as f32, |transform| transform.position[0]);
    let object_centre_x = anchor.x as f32 + (f32::from(definition.size()[0]) - 1.0) * 0.5;
    if object_centre_x < user_x {
        FurnitureFacing::Left
    } else {
        FurnitureFacing::Right
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
        ItemAction::PlaceCargoLift { object_type } => {
            furniture_definition(object_type).map(|definition| definition.size())
        }
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
    fn background_mode_maps_blocks_and_tools_without_affecting_other_items() {
        assert_eq!(
            ItemUseMode::Background.resolve(ItemAction::PlaceTile {
                layer: Layer::Foreground,
                tile: crate::ForegroundTile::DIRT,
            }),
            Some(ItemAction::PlaceTile {
                layer: Layer::Background,
                tile: crate::BackgroundTile::DIRT_WALL,
            })
        );
        assert_eq!(
            ItemUseMode::Background.resolve(ItemAction::Tool(ToolAction::RemoveTile {
                layer: Layer::Foreground,
                power: 3,
            })),
            Some(ItemAction::Tool(ToolAction::RemoveTile {
                layer: Layer::Background,
                power: 3,
            }))
        );
        assert_eq!(
            ItemUseMode::Background.resolve(ItemAction::PlaceTile {
                layer: Layer::Foreground,
                tile: TileId::new(4),
            }),
            None
        );
        assert_eq!(ItemUseMode::Background.resolve(ItemAction::PlaceRope), None);
    }

    #[test]
    fn directional_furniture_faces_away_from_the_placing_player() {
        let mut entities = EntityWorld::new();
        let player = entities.spawn((Transform::new([4.0, 4.0]),));
        assert_eq!(
            placement_facing(
                &entities,
                player,
                FurnitureObject::TURRET,
                TilePos::new(8, 4)
            ),
            FurnitureFacing::Right
        );
        assert_eq!(
            placement_facing(
                &entities,
                player,
                FurnitureObject::AMMO_TURRET,
                TilePos::new(1, 4)
            ),
            FurnitureFacing::Left
        );
        assert_eq!(
            placement_facing(
                &entities,
                player,
                FurnitureObject::CHEST,
                TilePos::new(1, 4)
            ),
            FurnitureFacing::Right
        );
    }

    #[test]
    fn placement_status_explains_valid_blocked_and_out_of_reach_targets() {
        const TEST_REACH: f32 = 8.0;
        let registry = ItemRegistry::with_built_ins();
        let inventory = Inventory::test_loadout(&registry);
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
            ItemTargetStatus::OutOfReach(TilePos::new(15, 4))
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
        let mut inventory = Inventory::test_loadout(&registry);
        inventory.select_hotbar(7);
        let mut terrain = World::empty(20, 20, 0).unwrap();
        for x in 7..=8 {
            terrain
                .set_tile(x, 6, Layer::Foreground, TileId::new(2))
                .unwrap();
        }
        let mut entities = EntityWorld::new();
        let user = entities.spawn((Transform::new([4.0, 3.0]), Collider::new(1.0, 2.0)));

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
        let mut inventory = Inventory::test_loadout(&registry);
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
    fn laser_bore_target_previews_its_four_by_three_footprint() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::test_loadout(&registry);
        inventory.select_hotbar(8);
        let mut terrain = World::empty(20, 20, 0).unwrap();
        for x in [7, 10] {
            terrain
                .set_tile(x, 7, Layer::Foreground, TileId::new(2))
                .unwrap();
        }
        let mut entities = EntityWorld::new();
        let user = entities.spawn((Transform::new([4.0, 3.0]), Collider::new(1.0, 2.0)));

        assert_eq!(
            selected_item_target_size(&inventory, &registry),
            Some([4, 3])
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
