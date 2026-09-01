use super::*;
use crate::{ForegroundTile, FurnitureObject, ItemId, Layer, TilePos, World};

#[test]
fn item_details_derive_function_from_typed_item_action() {
    let registry = ItemRegistry::with_built_ins();
    let potion = registry.get(ItemId::HEALING_POTION).unwrap();
    let pickaxe = registry.get(ItemId::PICKAXE).unwrap();
    assert_eq!(
        item_function(potion.action).as_deref(),
        Some("RESTORE 25 HEALTH")
    );
    assert_eq!(
        item_function(pickaxe.action).as_deref(),
        Some("MINE FOREGROUND BLOCKS (POWER 10)")
    );
}

#[test]
fn item_details_panel_stays_on_screen_with_lift_station_controls() {
    let viewport = [800.0, 600.0];
    let slots = Some(usize::from(crate::LIFT_STATION_SLOTS));
    let controls =
        FurnitureControls::from_interaction(crate::LIFT_STATION_DEFINITION.interaction());
    let layout = SlotLayout::with_details(viewport, slots, controls, true);
    let (centre, size) = layout.item_details_panel(slots, controls);

    assert!(centre[1] - size[1] * 0.5 >= SCREEN_MARGIN);
    assert!(centre[0] - size[0] * 0.5 >= SCREEN_MARGIN);
    assert!(centre[0] + size[0] * 0.5 <= viewport[0] - SCREEN_MARGIN);
}

#[test]
fn item_detail_text_is_truncated_to_the_panel_width() {
    let maximum_width = 70.0;
    let fitted = fit_text("ORBITAL EXPORT LAUNCHER", 1.5, maximum_width);
    assert!(fitted.ends_with("..."));
    assert!(GuiRenderer::text_width(&fitted, 1.5) <= maximum_width);
}

#[test]
fn closed_hotbar_click_selects_without_picking_up_stack() {
    let registry = ItemRegistry::with_built_ins();
    let mut inventory = Inventory::test_loadout(&registry);
    let mut gui = InventoryGui::default();
    let viewport = [800.0, 600.0];
    let second_slot = SlotLayout::new(viewport).position(1);
    assert!(gui.handle_click(
        second_slot,
        viewport,
        SlotClick::Primary,
        &mut inventory,
        None,
        &registry,
    ));
    assert_eq!(inventory.selected_hotbar(), 1);
    assert_eq!(inventory.slot(1).unwrap().item(), ItemId::STONE_BLOCK);
    assert_eq!(gui.cursor_stack(), None);
}

#[test]
fn expanded_inventory_click_picks_up_and_returns_a_stack() {
    let registry = ItemRegistry::with_built_ins();
    let mut inventory = Inventory::test_loadout(&registry);
    let mut gui = InventoryGui::default();
    assert!(gui.toggle(&mut inventory, &registry));
    let viewport = [800.0, 600.0];
    let first_slot = SlotLayout::new(viewport).position(0);
    gui.handle_click(
        first_slot,
        viewport,
        SlotClick::Primary,
        &mut inventory,
        None,
        &registry,
    );
    assert!(inventory.slot(0).is_none());
    assert!(gui.cursor_stack().is_some());
    assert!(gui.toggle(&mut inventory, &registry));
    assert!(!gui.is_open());
    assert!(gui.cursor_stack().is_none());
}

#[test]
fn crafting_tab_click_crafts_a_recipe_into_the_player_inventory() {
    let registry = ItemRegistry::with_built_ins();
    let mut inventory = Inventory::new();
    assert_eq!(inventory.add(ItemId::DIRT_BLOCK, 2, &registry), 0);
    let mut gui = InventoryGui::default();
    assert!(gui.toggle(&mut inventory, &registry));
    let viewport = [800.0, 600.0];

    let items_layout = SlotLayout::with_panel_height(
        viewport,
        None,
        FurnitureControls::default(),
        PERSONAL_ITEM_PANEL_HEIGHT,
    );
    let crafting_tab =
        CraftingLayout::new(items_layout, PERSONAL_ITEM_PANEL_HEIGHT).tab_buttons()[1];
    assert!(gui.handle_click(
        crafting_tab.0,
        viewport,
        SlotClick::Primary,
        &mut inventory,
        None,
        &registry,
    ));
    assert_eq!(gui.page, InventoryPage::Crafting);

    let crafting_layout = SlotLayout::with_panel_height(
        viewport,
        None,
        FurnitureControls::default(),
        CRAFTING_PANEL_HEIGHT,
    );
    let rope_recipe = CraftingLayout::new(crafting_layout, CRAFTING_PANEL_HEIGHT).recipe_button(0);
    assert!(gui.handle_click(
        rope_recipe.0,
        viewport,
        SlotClick::Primary,
        &mut inventory,
        None,
        &registry,
    ));

    assert_eq!(inventory.quantity(ItemId::DIRT_BLOCK), 0);
    assert_eq!(inventory.quantity(ItemId::ROPE), 5);
}

#[test]
fn crafting_panel_and_recipe_buttons_stay_clear_of_the_inventory_grid() {
    let viewport = [800.0, 600.0];
    let slot_layout = SlotLayout::with_panel_height(
        viewport,
        None,
        FurnitureControls::default(),
        CRAFTING_PANEL_HEIGHT,
    );
    let crafting = CraftingLayout::new(slot_layout, CRAFTING_PANEL_HEIGHT);
    let panel_left = crafting.panel.0[0] - crafting.panel.1[0] * 0.5;
    let panel_right = crafting.panel.0[0] + crafting.panel.1[0] * 0.5;
    let panel_top = crafting.panel.0[1] - crafting.panel.1[1] * 0.5;
    let panel_bottom = crafting.panel.0[1] + crafting.panel.1[1] * 0.5;
    let top_inventory_slot = slot_layout.position((crate::INVENTORY_ROWS - 1) * INVENTORY_COLUMNS);
    let grid_top = top_inventory_slot[1] - slot_layout.slot_size * 0.5;

    assert!(panel_left >= SCREEN_MARGIN);
    assert!(panel_right <= viewport[0] - SCREEN_MARGIN);
    assert!(panel_top >= SCREEN_MARGIN);
    assert!(panel_bottom < grid_top);
    for index in 0..CRAFTING_VISIBLE_RECIPES {
        let (centre, size) = crafting.recipe_button(index);
        assert!(centre[0] - size[0] * 0.5 >= panel_left);
        assert!(centre[0] + size[0] * 0.5 <= panel_right);
        assert!(centre[1] - size[1] * 0.5 >= panel_top);
        assert!(centre[1] + size[1] * 0.5 <= panel_bottom);
    }
    let filter = crafting.craftable_filter_button();
    let first_recipe = crafting.recipe_button(0);
    assert!(filter.0[1] + filter.1[1] * 0.5 < first_recipe.0[1] - first_recipe.1[1] * 0.5);
}

#[test]
fn crafting_scroll_moves_by_rows_and_stays_bounded() {
    let registry = ItemRegistry::with_built_ins();
    let mut inventory = Inventory::new();
    let mut gui = InventoryGui::default();
    assert!(gui.toggle(&mut inventory, &registry));
    gui.page = InventoryPage::Crafting;

    assert!(gui.scroll_crafting(-1.0, &inventory, &registry));
    assert_eq!(gui.crafting_offset, CRAFTING_COLUMNS);
    for _ in 0..20 {
        assert!(gui.scroll_crafting(-1.0, &inventory, &registry));
    }
    let maximum_offset = CRAFTING_RECIPES
        .len()
        .div_ceil(CRAFTING_COLUMNS)
        .saturating_sub(CRAFTING_VISIBLE_ROWS)
        * CRAFTING_COLUMNS;
    assert_eq!(gui.crafting_offset, maximum_offset);

    for _ in 0..20 {
        assert!(gui.scroll_crafting(1.0, &inventory, &registry));
    }
    assert_eq!(gui.crafting_offset, 0);
}

#[test]
fn can_craft_filter_hides_unavailable_recipes_and_preserves_click_mapping() {
    let registry = ItemRegistry::with_built_ins();
    let mut inventory = Inventory::new();
    assert_eq!(inventory.add(ItemId::DIRT_BLOCK, 2, &registry), 0);
    let mut gui = InventoryGui::default();
    assert!(gui.toggle(&mut inventory, &registry));
    gui.page = InventoryPage::Crafting;
    let viewport = [800.0, 600.0];
    let slot_layout = SlotLayout::with_panel_height(
        viewport,
        None,
        FurnitureControls::default(),
        CRAFTING_PANEL_HEIGHT,
    );
    let layout = CraftingLayout::new(slot_layout, CRAFTING_PANEL_HEIGHT);

    assert!(gui.handle_click(
        layout.craftable_filter_button().0,
        viewport,
        SlotClick::Primary,
        &mut inventory,
        None,
        &registry,
    ));
    assert!(gui.craftable_only);
    let recipes = filtered_crafting_recipes(&inventory, &registry, true);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].output.item, ItemId::ROPE);

    assert!(gui.handle_click(
        layout.recipe_button(0).0,
        viewport,
        SlotClick::Primary,
        &mut inventory,
        None,
        &registry,
    ));
    assert_eq!(inventory.quantity(ItemId::ROPE), 5);
}

#[test]
fn inventory_rows_are_arranged_above_the_hotbar() {
    let layout = SlotLayout::new([800.0, 600.0]);
    assert!(layout.position(HOTBAR_SLOTS)[1] < layout.position(0)[1]);
    assert!(layout.position(HOTBAR_SLOTS * crate::INVENTORY_ROWS - 1)[1] < layout.position(0)[1]);
}

#[test]
fn small_container_layout_only_reserves_its_visible_rows() {
    let viewport = [800.0, 300.0];
    let bore = SlotLayout::with_container(viewport, Some(usize::from(crate::LASER_BORE_SLOTS)));
    let chest = SlotLayout::with_container(viewport, Some(crate::CHEST_SLOTS));
    assert!(bore.slot_size > chest.slot_size);
}

#[test]
fn open_chest_moves_stacks_between_player_and_container_slots() {
    let registry = ItemRegistry::with_built_ins();
    let mut inventory = Inventory::test_loadout(&registry);
    let mut world = World::empty(8, 8, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 5, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
    }
    let chest = world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();
    let mut gui = InventoryGui::default();
    gui.show_container(chest);
    let viewport = [800.0, 600.0];
    let layout = SlotLayout::with_container(viewport, Some(crate::CHEST_SLOTS));

    assert!(gui.handle_click(
        layout.position(0),
        viewport,
        SlotClick::Primary,
        &mut inventory,
        world.container_mut(chest),
        &registry,
    ));
    assert!(gui.handle_click(
        layout.container_position(0),
        viewport,
        SlotClick::Primary,
        &mut inventory,
        world.container_mut(chest),
        &registry,
    ));

    assert!(inventory.slot(0).is_none());
    assert_eq!(
        world.container(chest).unwrap().slot(0).unwrap().item(),
        ItemId::DIRT_BLOCK
    );
    assert_eq!(gui.cursor_stack(), None);
}

#[test]
fn machine_activation_control_reports_its_object() {
    let mut gui = InventoryGui::default();
    let mut world = World::empty(12, 16, 0).unwrap();
    for x in [2, 5] {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let bore = world
        .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 3))
        .unwrap();
    gui.show_furniture(
        bore,
        "LASER BORE",
        crate::furniture_definition(FurnitureObject::LASER_BORE)
            .unwrap()
            .interaction(),
    );
    let viewport = [800.0, 600.0];
    let slots = usize::from(crate::LASER_BORE_SLOTS);
    let controls = FurnitureControls {
        activation: true,
        target_priority: false,
        laser_aim: false,
        battery_status: false,
        drill_depth_status: true,
        subsurface_survey_status: false,
        kill_count_status: false,
        lift_controls: false,
        lift_station_controls: false,
    };
    let layout = SlotLayout::with_furniture(viewport, Some(slots), controls);
    let (button, _) = layout.activation_button(Some(slots), controls);

    assert_eq!(
        gui.control_action_at(button, viewport, Some(slots), false),
        Some(FurnitureControlAction::SetActive(bore, true))
    );
    assert_eq!(
        gui.control_action_at([0.0, 0.0], viewport, Some(slots), false),
        None
    );
}

#[test]
fn processor_manual_slots_accept_inputs_and_protect_output() {
    let registry = ItemRegistry::with_built_ins();
    let mut inventory = Inventory::new();
    let mut container = ItemContainer::new(usize::from(crate::COMPOSITE_ASSEMBLER_SLOTS));
    let mut gui = InventoryGui::default();
    let interaction = crate::COMPOSITE_ASSEMBLER_DEFINITION.interaction();
    let mut world = World::empty(20, 20, 0).unwrap();
    for x in 4..=6 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let processor = world
        .place_furniture(FurnitureObject::COMPOSITE_ASSEMBLER, TilePos::new(4, 6))
        .unwrap();
    gui.show_furniture(processor, "RESOURCE PROCESSOR", interaction);
    assert!(gui.processor_slots);

    let viewport = [800.0, 600.0];
    let controls = FurnitureControls::from_interaction(interaction);
    let layout = SlotLayout::with_furniture(
        viewport,
        Some(usize::from(crate::COMPOSITE_ASSEMBLER_SLOTS)),
        controls,
    );

    gui.restore_cursor_stack(ItemStack::new(ItemId::STONE_BLOCK, 1));
    assert!(gui.handle_click(
        layout.container_position(0),
        viewport,
        SlotClick::Primary,
        &mut inventory,
        Some(&mut container),
        &registry,
    ));
    assert!(container.slot(0).is_none());
    assert!(gui.cursor_stack().is_some());

    assert!(gui.handle_click(
        layout.container_position(1),
        viewport,
        SlotClick::Primary,
        &mut inventory,
        Some(&mut container),
        &registry,
    ));
    assert_eq!(container.slot(1).unwrap().item(), ItemId::STONE_BLOCK);
    assert!(gui.cursor_stack().is_none());

    gui.restore_cursor_stack(ItemStack::new(ItemId::IRON_ORE, 1));
    assert!(gui.handle_click(
        layout.container_position(2),
        viewport,
        SlotClick::Primary,
        &mut inventory,
        Some(&mut container),
        &registry,
    ));
    assert!(container.slot(2).is_none());
    assert_eq!(gui.cursor_stack().unwrap().item(), ItemId::IRON_ORE);
}

#[test]
fn advanced_targeting_controls_require_the_security_bonus() {
    let mut gui = InventoryGui::default();
    let mut world = World::empty(12, 16, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let turret = world
        .place_furniture(FurnitureObject::TURRET, TilePos::new(2, 4))
        .unwrap();
    let interaction = crate::furniture_definition(FurnitureObject::TURRET)
        .unwrap()
        .interaction();
    gui.show_furniture(turret, "DEFENCE TURRET", interaction);
    let viewport = [800.0, 600.0];
    let controls = FurnitureControls::from_interaction(interaction);
    let layout = SlotLayout::with_furniture(viewport, None, controls);
    let strongest = layout.target_buttons(None, controls)[1];

    assert_eq!(
        gui.control_action_at(strongest.0, viewport, None, false),
        None
    );

    gui.show_furniture_with_targeting(turret, "DEFENCE TURRET", interaction, true);
    assert_eq!(
        gui.control_action_at(strongest.0, viewport, None, false),
        Some(FurnitureControlAction::SetTargetPriority(
            turret,
            TargetPriority::Strongest
        ))
    );
}

#[test]
fn laser_drill_interaction_exposes_seven_distinct_aim_presets() {
    let mut gui = InventoryGui::default();
    let mut world = World::empty(48, 24, 0).unwrap();
    for x in [20, 22] {
        world
            .set_tile(x, 10, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let drill = world
        .place_furniture(FurnitureObject::LASER_DRILL, TilePos::new(20, 8))
        .unwrap();
    let interaction = crate::LASER_DRILL_DEFINITION.interaction();
    gui.show_furniture(drill, "LASER DRILL", interaction);
    let viewport = [800.0, 600.0];
    let controls = FurnitureControls::from_interaction(interaction);
    let slots = Some(usize::from(crate::LASER_DRILL_SLOTS));
    let layout = SlotLayout::with_furniture(viewport, slots, controls);
    let buttons = layout.laser_aim_buttons(slots, controls);

    assert!(controls.laser_aim);
    assert_eq!(buttons.len(), LaserDrillAim::ALL.len());
    assert_eq!(
        gui.control_action_at(buttons[0].0, viewport, slots, false),
        Some(FurnitureControlAction::SetLaserDrillAim(
            drill,
            LaserDrillAim::FarLeft
        ))
    );
    assert_eq!(
        gui.control_action_at(buttons[6].0, viewport, slots, false),
        Some(FurnitureControlAction::SetLaserDrillAim(
            drill,
            LaserDrillAim::FarRight
        ))
    );
}

#[test]
fn battery_status_reserves_a_read_only_meter() {
    let interaction = crate::BATTERY_DEFINITION.interaction();
    let controls = FurnitureControls::from_interaction(interaction);
    let layout = SlotLayout::with_furniture([800.0, 600.0], None, controls);
    let (_, size) = layout.battery_meter(None, controls);

    assert!(controls.battery_status);
    assert!(!controls.activation);
    assert!(!controls.target_priority);
    assert!(size[0] > 0.0);
    assert_eq!(BatteryStatus::new(240_000, 480_000).fraction(), 0.5);
    assert_eq!(BatteryStatus::new(600_000, 480_000).fraction(), 1.0);
}

#[test]
fn machine_statistics_are_definition_owned_and_format_depth_consistently() {
    let bore = FurnitureControls::from_interaction(crate::LASER_BORE_DEFINITION.interaction());
    let turret = FurnitureControls::from_interaction(crate::TURRET_DEFINITION.interaction());

    assert!(bore.drill_depth_status);
    assert!(!bore.kill_count_status);
    assert!(turret.kill_count_status);
    assert!(!turret.drill_depth_status);
    assert_eq!(signed_metres(427), "+42.7M");
    assert_eq!(signed_metres(-35), "-3.5M");

    let layout = SlotLayout::with_furniture([800.0, 600.0], None, turret);
    let status = layout.status_line(None, turret, 0);
    let control_centre =
        layout.target_buttons(None, turret)[0].0[0] + layout.target_buttons(None, turret)[3].0[0];
    assert!((status[0] - control_centre * 0.5).abs() < f32::EPSILON);
}

#[test]
fn subsurface_surveyor_reserves_its_readout_area() {
    let controls =
        FurnitureControls::from_interaction(crate::SUBSURFACE_SURVEYOR_DEFINITION.interaction());
    assert!(controls.activation);
    assert!(controls.subsurface_survey_status);
    assert!(!controls.drill_depth_status);
    assert_eq!(controls.height(), 124.0);
}

#[test]
fn cargo_lift_ui_exposes_distinct_up_and_down_commands() {
    let mut world = World::empty(16, 20, 0).unwrap();
    world
        .set_tile(6, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(6, 3))
        .unwrap();
    for _ in 0..6 {
        world
            .place_or_extend_powered_cable(TilePos::new(6, 3))
            .unwrap();
    }
    let lift = world.place_cargo_lift(TilePos::new(6, 4)).unwrap();
    let interaction = crate::CARGO_LIFT_DEFINITION.interaction();
    let controls = FurnitureControls::from_interaction(interaction);
    let slots = Some(usize::from(crate::CARGO_LIFT_SLOTS));
    let viewport = [800.0, 600.0];
    let layout = SlotLayout::with_furniture(viewport, slots, controls);
    let buttons = layout.lift_buttons(slots, controls);
    let mut gui = InventoryGui::default();
    gui.show_furniture(lift, "CARGO LIFT", interaction);

    assert!(controls.lift_controls);
    assert_eq!(
        gui.control_action_at(buttons[0].0, viewport, slots, true),
        Some(FurnitureControlAction::MoveCargoLift(
            lift,
            CargoLiftDirection::Up
        ))
    );
    assert_eq!(
        gui.control_action_at(buttons[1].0, viewport, slots, true),
        Some(FurnitureControlAction::MoveCargoLift(
            lift,
            CargoLiftDirection::Down
        ))
    );
}

#[test]
fn lift_station_ui_exposes_transfer_and_departure_settings() {
    let mut world = World::empty(16, 20, 0).unwrap();
    world
        .set_tile(6, 2, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    world
        .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(6, 3))
        .unwrap();
    for _ in 0..6 {
        world
            .place_or_extend_powered_cable(TilePos::new(6, 3))
            .unwrap();
    }
    for x in 7..=8 {
        world
            .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let station = world.place_lift_station(TilePos::new(7, 6)).unwrap();
    let interaction = crate::LIFT_STATION_DEFINITION.interaction();
    let controls = FurnitureControls::from_interaction(interaction);
    let slots = Some(usize::from(crate::LIFT_STATION_SLOTS));
    let viewport = [800.0, 600.0];
    let layout = SlotLayout::with_furniture(viewport, slots, controls);
    let [modes, departures] = layout.lift_station_buttons(slots, controls);
    let mut gui = InventoryGui::default();
    gui.show_furniture(station, "LIFT STATION", interaction);

    assert!(controls.lift_station_controls);
    assert_eq!(
        gui.control_action_at(modes[1].0, viewport, slots, true),
        Some(FurnitureControlAction::SetLiftStationMode(
            station,
            LiftStationMode::Unload
        ))
    );
    assert_eq!(
        gui.control_action_at(departures[0].0, viewport, slots, true),
        Some(FurnitureControlAction::SetLiftStationDeparture(
            station,
            CargoLiftDirection::Up
        ))
    );
}
