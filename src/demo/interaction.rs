use super::input::InputState;
use deep_tek::{
    Collider, DEFAULT_ITEM_REACH, DroppedItemSystem, EffectsMaterials, FollowCamera,
    FurnitureControlAction, Inventory, InventoryGui, ItemAction, ItemId, ItemRegistry,
    ItemTargetStatus, ItemUseResult, ObjectId, ObjectTypeId, PowerSystem, ProcurementGui,
    ProjectileKind, SlotClick, Specialist, SpecialistGui, SpecialistId, TerrainRenderer, TilePos,
    Transform, World, entity_position, furniture_definition, selected_item_target_size,
    selected_item_target_status, spawn_bomb, spawn_glowstick, use_selected_item,
    use_selected_item_in_background,
};
use easy_gpu::{assets::Material, assets_manager::Handle};
use hecs::{Entity, World as EntityWorld};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

const CONTINUOUS_ITEM_USE_INTERVAL: Duration = Duration::from_millis(75);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorldAction {
    Mined(ItemId),
    Placed(ObjectTypeId),
    Sleep(ObjectId),
    TerminalUnpowered,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_pointer_actions(
    input: &mut InputState,
    camera: &FollowCamera,
    viewport: [f32; 2],
    inventory_gui: &mut InventoryGui,
    procurement_gui: &mut ProcurementGui,
    specialist_gui: &mut SpecialistGui,
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    world: &mut World,
    power: &PowerSystem,
    renderer: &mut TerrainRenderer,
    dropped_items: &mut DroppedItemSystem,
    dropped_item_material: Option<Handle<Material>>,
    entities: &mut EntityWorld,
    player: Option<Entity>,
    effects_materials: Option<EffectsMaterials>,
    world_actions: &mut Vec<WorldAction>,
) {
    if std::mem::take(&mut input.interaction_queued) {
        let world_position = camera.screen_to_world(input.cursor_position, viewport);
        let specialist = specialist_at(entities, player, world_position);
        let mut interacted = false;
        if let Some(specialist) = specialist {
            inventory_gui.dismiss();
            procurement_gui.dismiss();
            specialist_gui.show(specialist);
            input.clear_focus();
            interacted = true;
        } else if let Some(target) = world_tile_at(world_position, world) {
            interacted = handle_furniture_interaction(
                procurement_gui,
                specialist_gui,
                inventory_gui,
                input,
                world,
                power,
                target,
                world_actions,
            );
        }
        if !interacted {
            inventory_gui.toggle(inventory, registry);
        }
    }
    if let Some(click) = input.primary_click_queued.take() {
        let open_container = inventory_gui.open_container();
        let container_slots = open_container
            .and_then(|object| world.container(object))
            .map(|container| container.slots().len());
        let active = open_container.is_some_and(|object| {
            world
                .object(object)
                .is_some_and(|object| object.is_active())
        });
        let control =
            inventory_gui.control_action_at(click.pixel, viewport, container_slots, active);
        let consumed = if let Some(action) = control {
            match action {
                FurnitureControlAction::SetActive(object, active) => {
                    world.set_furniture_active(object, active);
                }
                FurnitureControlAction::SetTargetPriority(object, priority) => {
                    world.set_furniture_target_priority(object, priority);
                }
                FurnitureControlAction::SetLaserDrillAim(object, aim) => {
                    world.set_laser_drill_aim(object, aim);
                }
                FurnitureControlAction::MoveCargoLift(object, direction) => {
                    world.set_cargo_lift_direction(object, direction);
                }
                FurnitureControlAction::SetLiftStationMode(object, mode) => {
                    world.set_lift_station_mode(object, mode);
                }
                FurnitureControlAction::SetLiftStationDeparture(object, direction) => {
                    world.set_lift_station_departure(object, direction);
                }
            }
            true
        } else {
            inventory_gui.handle_click(
                click.pixel,
                viewport,
                SlotClick::Primary,
                inventory,
                open_container.and_then(|object| world.container_mut(object)),
                registry,
            )
        };
        if !consumed {
            use_item_at(
                inventory,
                registry,
                world,
                renderer,
                dropped_items,
                dropped_item_material,
                entities,
                player,
                effects_materials,
                click.world,
                world_actions,
            );
            input.primary_world_use_active = true;
            input.last_continuous_item_use = Instant::now();
        } else {
            input.primary_world_use_active = false;
        }
    }
    if let Some(click) = input.secondary_click_queued.take() {
        let open_container = inventory_gui.open_container();
        let consumed = inventory_gui.handle_click(
            click.pixel,
            viewport,
            SlotClick::Secondary,
            inventory,
            open_container.and_then(|object| world.container_mut(object)),
            registry,
        );
        if !consumed {
            use_background_item_at(
                inventory,
                registry,
                world,
                renderer,
                dropped_items,
                dropped_item_material,
                entities,
                player,
                click.world,
            );
            input.secondary_world_use_active = true;
            input.last_continuous_secondary_use = Instant::now();
        } else {
            input.secondary_world_use_active = false;
        }
    }
    let pointer_captured =
        inventory_gui.captures_pointer(input.cursor_position, viewport, inventory);
    if input.primary_down
        && input.primary_world_use_active
        && !pointer_captured
        && selected_item_supports_continuous_use(inventory, registry)
        && input.last_continuous_item_use.elapsed() >= CONTINUOUS_ITEM_USE_INTERVAL
    {
        let world_position = camera.screen_to_world(input.cursor_position, viewport);
        use_item_at(
            inventory,
            registry,
            world,
            renderer,
            dropped_items,
            dropped_item_material,
            entities,
            player,
            effects_materials,
            world_position,
            world_actions,
        );
        input.last_continuous_item_use = Instant::now();
    }
    if input.secondary_down
        && input.secondary_world_use_active
        && !pointer_captured
        && selected_item_supports_background_use(inventory, registry)
        && input.last_continuous_secondary_use.elapsed() >= CONTINUOUS_ITEM_USE_INTERVAL
    {
        let world_position = camera.screen_to_world(input.cursor_position, viewport);
        use_background_item_at(
            inventory,
            registry,
            world,
            renderer,
            dropped_items,
            dropped_item_material,
            entities,
            player,
            world_position,
        );
        input.last_continuous_secondary_use = Instant::now();
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_furniture_interaction(
    procurement_gui: &mut ProcurementGui,
    specialist_gui: &mut SpecialistGui,
    inventory_gui: &mut InventoryGui,
    input: &mut InputState,
    world: &mut World,
    power: &PowerSystem,
    target: TilePos,
    world_actions: &mut Vec<WorldAction>,
) -> bool {
    let Some((object, interaction)) = world
        .furniture_interaction_at(target)
        .filter(|(_, interaction)| interaction.is_interactive())
    else {
        return false;
    };
    let Some(definition) = world
        .object(object)
        .and_then(|object| furniture_definition(object.object_type()))
    else {
        return false;
    };
    if interaction.toggles_door() {
        specialist_gui.dismiss();
        world.toggle_door(object);
        procurement_gui.dismiss();
        inventory_gui.dismiss();
        input.clear_focus();
    } else if interaction.opens_procurement() {
        specialist_gui.dismiss();
        inventory_gui.dismiss();
        if power.is_powered(object) {
            procurement_gui.show_for(object);
        } else {
            procurement_gui.dismiss();
            world_actions.push(WorldAction::TerminalUnpowered);
        }
        input.clear_focus();
    } else if interaction.allows_sleep() {
        specialist_gui.dismiss();
        procurement_gui.dismiss();
        inventory_gui.dismiss();
        input.clear_focus();
        world_actions.push(WorldAction::Sleep(object));
    } else {
        specialist_gui.dismiss();
        procurement_gui.dismiss();
        inventory_gui.show_furniture_with_targeting(
            object,
            definition.name(),
            interaction,
            world.specialist_bonuses().advanced_turret_targeting(),
        );
    }
    true
}

fn specialist_at(
    entities: &EntityWorld,
    player: Option<Entity>,
    world_position: [f32; 2],
) -> Option<SpecialistId> {
    let player_position = player.and_then(|player| entity_position(entities, player))?;
    let reach_squared = DEFAULT_ITEM_REACH * DEFAULT_ITEM_REACH;
    if distance_squared(player_position, world_position) > reach_squared {
        return None;
    }
    entities
        .query::<(&Specialist, &Transform, &Collider)>()
        .iter()
        .filter(|(_, (_, _, collider))| collider.enabled)
        .filter_map(|(_, (specialist, transform, collider))| {
            let centre = [
                transform.position[0] + collider.offset[0],
                transform.position[1] + collider.offset[1],
            ];
            let inside = (world_position[0] - centre[0]).abs() <= collider.half_extents[0]
                && (world_position[1] - centre[1]).abs() <= collider.half_extents[1];
            inside.then_some((distance_squared(centre, world_position), specialist.id))
        })
        .min_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, id)| id)
}

fn distance_squared(left: [f32; 2], right: [f32; 2]) -> f32 {
    (left[0] - right[0]).powi(2) + (left[1] - right[1]).powi(2)
}

#[allow(clippy::too_many_arguments)]
fn use_item_at(
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    world: &mut World,
    renderer: &mut TerrainRenderer,
    dropped_items: &mut DroppedItemSystem,
    dropped_item_material: Option<Handle<Material>>,
    entities: &mut EntityWorld,
    player: Option<Entity>,
    effects_materials: Option<EffectsMaterials>,
    world_position: [f32; 2],
    world_actions: &mut Vec<WorldAction>,
) {
    let Some(player) = player else {
        return;
    };
    let Some(dropped_item_material) = dropped_item_material else {
        return;
    };
    let target = world_tile_at(world_position, world);
    let mined_item = target
        .and_then(|target| {
            world
                .tile(target.x, target.y, deep_tek::Layer::Foreground)
                .ok()
        })
        .and_then(|tile| registry.item_for_tile(deep_tek::Layer::Foreground, tile));
    let selected_action = inventory
        .selected_stack()
        .and_then(|stack| registry.get(stack.item()))
        .map(|definition| definition.action);
    let result = use_selected_item(
        inventory,
        registry,
        world,
        renderer,
        dropped_items,
        dropped_item_material,
        entities,
        player,
        target,
        DEFAULT_ITEM_REACH,
    );
    match (result, selected_action) {
        (ItemUseResult::Removed(_), _) => {
            if let Some(item) = mined_item {
                world_actions.push(WorldAction::Mined(item));
            }
        }
        (ItemUseResult::Placed(_), Some(ItemAction::PlaceFurniture { object_type })) => {
            world_actions.push(WorldAction::Placed(object_type));
        }
        _ => {}
    }
    if let ItemUseResult::Thrown(projectile) = result
        && let (Some(materials), Some(origin)) =
            (effects_materials, entity_position(entities, player))
    {
        match projectile {
            ProjectileKind::GlowStick => {
                spawn_glowstick(entities, materials.projectile, origin, world_position);
            }
            ProjectileKind::Bomb => {
                spawn_bomb(entities, materials.projectile, origin, world_position);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn use_background_item_at(
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    world: &mut World,
    renderer: &mut TerrainRenderer,
    dropped_items: &mut DroppedItemSystem,
    dropped_item_material: Option<Handle<Material>>,
    entities: &mut EntityWorld,
    player: Option<Entity>,
    world_position: [f32; 2],
) {
    let (Some(player), Some(dropped_item_material)) = (player, dropped_item_material) else {
        return;
    };
    use_selected_item_in_background(
        inventory,
        registry,
        world,
        renderer,
        dropped_items,
        dropped_item_material,
        entities,
        player,
        world_tile_at(world_position, world),
        DEFAULT_ITEM_REACH,
    );
}

pub(crate) fn selected_item_supports_continuous_use(
    inventory: &Inventory,
    registry: &ItemRegistry,
) -> bool {
    inventory
        .selected_stack()
        .and_then(|stack| registry.get(stack.item()))
        .is_some_and(|definition| definition.action.supports_continuous_use())
}

fn selected_item_supports_background_use(inventory: &Inventory, registry: &ItemRegistry) -> bool {
    inventory
        .selected_stack()
        .and_then(|stack| registry.get(stack.item()))
        .is_some_and(|definition| match definition.action {
            deep_tek::ItemAction::PlaceTile { tile, .. } => {
                deep_tek::background_tile_for(tile).is_some()
            }
            deep_tek::ItemAction::Tool(_) => true,
            _ => false,
        })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn target_preview(
    input: &InputState,
    camera: &FollowCamera,
    viewport: [f32; 2],
    inventory_gui: &InventoryGui,
    inventory: &Inventory,
    registry: &ItemRegistry,
    world: &World,
    entities: &EntityWorld,
    player: Option<Entity>,
) -> Option<([f32; 2], [f32; 2], [f32; 4])> {
    let size = selected_item_target_size(inventory, registry)?;
    if inventory_gui.captures_pointer(input.cursor_position, viewport, inventory) {
        return None;
    }
    let target = world_tile_at(
        camera.screen_to_world(input.cursor_position, viewport),
        world,
    )?;
    let status = selected_item_target_status(
        inventory,
        registry,
        world,
        entities,
        player?,
        Some(target),
        DEFAULT_ITEM_REACH,
    );
    let (anchor, tint) = match status {
        ItemTargetStatus::Valid(anchor) => (anchor, [0.35, 1.0, 0.35, 0.7]),
        ItemTargetStatus::Blocked(anchor) | ItemTargetStatus::OutOfReach(anchor) => {
            (anchor, [1.0, 0.3, 0.25, 0.7])
        }
        ItemTargetStatus::NotTargeted => return None,
    };
    let tile_pixels = viewport[1] / camera.vertical_tiles_visible();
    let centre = [
        anchor.x as f32 + (f32::from(size[0]) - 1.0) * 0.5,
        anchor.y as f32 + (f32::from(size[1]) - 1.0) * 0.5,
    ];
    Some((
        camera.world_to_screen(centre, viewport),
        [
            tile_pixels * f32::from(size[0]),
            tile_pixels * f32::from(size[1]),
        ],
        tint,
    ))
}

pub(crate) fn world_tile_at(position: [f32; 2], world: &World) -> Option<TilePos> {
    if !position[0].is_finite() || !position[1].is_finite() {
        return None;
    }
    let x = position[0].round() as i64;
    let y = position[1].round() as i64;
    (x >= 0 && y >= 0 && x < i64::from(world.width()) && y < i64::from(world.height()))
        .then(|| TilePos::new(x as u32, y as u32))
}

pub(crate) fn hotbar_slot_for_key(key: KeyCode) -> Option<usize> {
    match key {
        KeyCode::Digit1 => Some(0),
        KeyCode::Digit2 => Some(1),
        KeyCode::Digit3 => Some(2),
        KeyCode::Digit4 => Some(3),
        KeyCode::Digit5 => Some(4),
        KeyCode::Digit6 => Some(5),
        KeyCode::Digit7 => Some(6),
        KeyCode::Digit8 => Some(7),
        KeyCode::Digit9 => Some(8),
        KeyCode::Digit0 => Some(9),
        _ => None,
    }
}
