use super::input::InputState;
use deep_tek::{
    DEFAULT_ITEM_REACH, EffectsMaterials, FollowCamera, FurnitureControlAction, Inventory,
    InventoryGui, ItemRegistry, ItemTargetStatus, ItemUseResult, ProjectileKind, SlotClick,
    TerrainRenderer, TilePos, World, entity_position, furniture_definition,
    selected_item_target_size, selected_item_target_status, spawn_bomb, spawn_glowstick,
    use_selected_item,
};
use hecs::{Entity, World as EntityWorld};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

const CONTINUOUS_ITEM_USE_INTERVAL: Duration = Duration::from_millis(75);

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_pointer_actions(
    input: &mut InputState,
    camera: &FollowCamera,
    viewport: [f32; 2],
    inventory_gui: &mut InventoryGui,
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    world: &mut World,
    renderer: &mut TerrainRenderer,
    entities: &mut EntityWorld,
    player: Option<Entity>,
    effects_materials: Option<EffectsMaterials>,
) {
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
                entities,
                player,
                effects_materials,
                click.world,
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
        if !consumed
            && let Some(target) = world_tile_at(click.world, world)
            && let Some((object, interaction)) = world.furniture_interaction_at(target)
            && interaction.is_interactive()
            && let Some(definition) = world
                .object(object)
                .and_then(|object| furniture_definition(object.object_type()))
        {
            inventory_gui.show_furniture(object, definition.name(), interaction);
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
            entities,
            player,
            effects_materials,
            world_position,
        );
        input.last_continuous_item_use = Instant::now();
    }
}

#[allow(clippy::too_many_arguments)]
fn use_item_at(
    inventory: &mut Inventory,
    registry: &ItemRegistry,
    world: &mut World,
    renderer: &mut TerrainRenderer,
    entities: &mut EntityWorld,
    player: Option<Entity>,
    effects_materials: Option<EffectsMaterials>,
    world_position: [f32; 2],
) {
    let Some(player) = player else {
        return;
    };
    let result = use_selected_item(
        inventory,
        registry,
        world,
        renderer,
        entities,
        player,
        world_tile_at(world_position, world),
        DEFAULT_ITEM_REACH,
    );
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

pub(crate) fn selected_item_supports_continuous_use(
    inventory: &Inventory,
    registry: &ItemRegistry,
) -> bool {
    inventory
        .selected_stack()
        .and_then(|stack| registry.get(stack.item()))
        .is_some_and(|definition| definition.action.supports_continuous_use())
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
