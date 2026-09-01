use super::*;

impl InventoryGui {
    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub const fn cursor_stack(&self) -> Option<ItemStack> {
        self.cursor_stack
    }

    pub fn restore_cursor_stack(&mut self, stack: Option<ItemStack>) {
        self.cursor_stack = stack;
    }

    pub const fn open_container(&self) -> Option<ObjectId> {
        self.open_container
    }

    /// Closes inventory or furniture UI without discarding a held cursor stack.
    /// The stack remains attached to the cursor and is visible again next time
    /// the inventory opens.
    pub fn dismiss(&mut self) {
        self.open = false;
        self.open_container = None;
        self.open_title = None;
        self.controls = FurnitureControls::default();
        self.page = InventoryPage::Items;
        self.crafting_offset = 0;
        self.advanced_targeting = false;
        self.processor_slots = false;
    }

    pub fn show_container(&mut self, object: ObjectId) {
        self.open = true;
        self.open_container = Some(object);
        self.open_title = Some("CONTAINER");
        self.controls = FurnitureControls::default();
        self.page = InventoryPage::Items;
        self.crafting_offset = 0;
        self.advanced_targeting = false;
        self.processor_slots = false;
    }

    pub fn show_furniture(
        &mut self,
        object: ObjectId,
        title: &'static str,
        interaction: FurnitureInteraction,
    ) {
        self.show_furniture_with_targeting(object, title, interaction, false);
    }

    pub fn show_furniture_with_targeting(
        &mut self,
        object: ObjectId,
        title: &'static str,
        interaction: FurnitureInteraction,
        advanced_targeting: bool,
    ) {
        self.open = true;
        self.open_container = Some(object);
        self.open_title = Some(title);
        self.controls = FurnitureControls::from_interaction(interaction);
        self.page = InventoryPage::Items;
        self.crafting_offset = 0;
        self.advanced_targeting = advanced_targeting;
        self.processor_slots =
            interaction.item_transport_role() == Some(ItemTransportRole::Processor);
    }

    /// Returns the open object when the reusable machine control contains the
    /// cursor. State changes remain owned by the world rather than the GUI.
    pub fn control_action_at(
        &self,
        cursor: [f32; 2],
        viewport: [f32; 2],
        container_slots: Option<usize>,
        active: bool,
    ) -> Option<FurnitureControlAction> {
        let object = self.open_container?;
        let layout = SlotLayout::with_details(viewport, container_slots, self.controls, self.open);
        if self.controls.activation {
            let (centre, size) = layout.activation_button(container_slots, self.controls);
            if rect_contains(centre, size, cursor) {
                return Some(FurnitureControlAction::SetActive(object, !active));
            }
        }
        if self.controls.target_priority {
            for (priority, (centre, size)) in TargetPriority::ALL
                .into_iter()
                .zip(layout.target_buttons(container_slots, self.controls))
            {
                if !self.advanced_targeting && priority != TargetPriority::Closest {
                    continue;
                }
                if rect_contains(centre, size, cursor) {
                    return Some(FurnitureControlAction::SetTargetPriority(object, priority));
                }
            }
        }
        if self.controls.laser_aim {
            for (aim, (centre, size)) in LaserDrillAim::ALL
                .into_iter()
                .zip(layout.laser_aim_buttons(container_slots, self.controls))
            {
                if rect_contains(centre, size, cursor) {
                    return Some(FurnitureControlAction::SetLaserDrillAim(object, aim));
                }
            }
        }
        if self.controls.lift_controls {
            let [up, down] = layout.lift_buttons(container_slots, self.controls);
            if rect_contains(up.0, up.1, cursor) {
                return Some(FurnitureControlAction::MoveCargoLift(
                    object,
                    CargoLiftDirection::Up,
                ));
            }
            if rect_contains(down.0, down.1, cursor) {
                return Some(FurnitureControlAction::MoveCargoLift(
                    object,
                    CargoLiftDirection::Down,
                ));
            }
        }
        if self.controls.lift_station_controls {
            let [modes, departures] = layout.lift_station_buttons(container_slots, self.controls);
            for (mode, button) in LiftStationMode::ALL.into_iter().zip(modes) {
                if rect_contains(button.0, button.1, cursor) {
                    return Some(FurnitureControlAction::SetLiftStationMode(object, mode));
                }
            }
            for (direction, button) in [CargoLiftDirection::Up, CargoLiftDirection::Down]
                .into_iter()
                .zip(departures)
            {
                if rect_contains(button.0, button.1, cursor) {
                    return Some(FurnitureControlAction::SetLiftStationDeparture(
                        object, direction,
                    ));
                }
            }
        }
        None
    }

    pub fn captures_pointer(
        &self,
        cursor: [f32; 2],
        viewport: [f32; 2],
        inventory: &Inventory,
    ) -> bool {
        if self.open {
            return true;
        }
        let layout = SlotLayout::new(viewport);
        (0..HOTBAR_SLOTS.min(inventory.slots().len())).any(|slot| layout.contains(slot, cursor))
    }

    /// Returns false only when closing would strand a cursor stack in a full inventory.
    pub fn toggle(&mut self, inventory: &mut Inventory, registry: &ItemRegistry) -> bool {
        if !self.open {
            self.open = true;
            self.open_container = None;
            self.open_title = None;
            self.controls = FurnitureControls::default();
            self.page = InventoryPage::Items;
            self.crafting_offset = 0;
            self.advanced_targeting = false;
            self.processor_slots = false;
            return true;
        }
        if let Some(stack) = self.cursor_stack.take() {
            let remaining = inventory.add(stack.item(), stack.quantity(), registry);
            if remaining > 0 {
                self.cursor_stack = ItemStack::new(stack.item(), remaining);
                return false;
            }
        }
        self.dismiss();
        true
    }

    /// Scrolls the personal crafting recipe grid by one row. Returning true
    /// prevents the same wheel input from changing the selected hotbar slot.
    pub fn scroll_crafting(
        &mut self,
        direction: f32,
        inventory: &Inventory,
        registry: &ItemRegistry,
    ) -> bool {
        if !self.open || self.open_container.is_some() || self.page != InventoryPage::Crafting {
            return false;
        }
        let recipe_count =
            filtered_crafting_recipes(inventory, registry, self.craftable_only).len();
        let total_rows = recipe_count.div_ceil(CRAFTING_COLUMNS);
        let maximum_offset = total_rows
            .saturating_sub(CRAFTING_VISIBLE_ROWS)
            .saturating_mul(CRAFTING_COLUMNS);
        if direction > 0.0 {
            self.crafting_offset = self.crafting_offset.saturating_sub(CRAFTING_COLUMNS);
        } else if direction < 0.0 {
            self.crafting_offset = (self.crafting_offset + CRAFTING_COLUMNS).min(maximum_offset);
        }
        true
    }

    /// Handles a pixel-space click and reports whether the GUI consumed it.
    pub fn handle_click(
        &mut self,
        cursor: [f32; 2],
        viewport: [f32; 2],
        click: SlotClick,
        inventory: &mut Inventory,
        mut container: Option<&mut ItemContainer>,
        registry: &ItemRegistry,
    ) -> bool {
        let container_slots = container
            .as_deref()
            .map(|container| container.slots().len());
        let personal_inventory = self.open && self.open_container.is_none();
        let panel_height = if personal_inventory {
            self.page.panel_height()
        } else if self.open {
            ITEM_DETAILS_HEIGHT
        } else {
            0.0
        };
        let layout =
            SlotLayout::with_panel_height(viewport, container_slots, self.controls, panel_height);
        if personal_inventory {
            let crafting_layout = CraftingLayout::new(layout, panel_height);
            let [items_tab, crafting_tab] = crafting_layout.tab_buttons();
            if rect_contains(items_tab.0, items_tab.1, cursor) {
                self.page = InventoryPage::Items;
                return true;
            }
            if rect_contains(crafting_tab.0, crafting_tab.1, cursor) {
                self.page = InventoryPage::Crafting;
                return true;
            }
            if self.page == InventoryPage::Crafting
                && rect_contains(
                    crafting_layout.craftable_filter_button().0,
                    crafting_layout.craftable_filter_button().1,
                    cursor,
                )
            {
                self.craftable_only = !self.craftable_only;
                self.crafting_offset = 0;
                return true;
            }
            if self.page == InventoryPage::Crafting
                && let Some(recipe) =
                    filtered_crafting_recipes(inventory, registry, self.craftable_only)
                        .into_iter()
                        .skip(self.crafting_offset)
                        .take(CRAFTING_VISIBLE_RECIPES)
                        .enumerate()
                        .find_map(|(index, recipe)| {
                            let button = crafting_layout.recipe_button(index);
                            rect_contains(button.0, button.1, cursor).then_some(recipe)
                        })
            {
                if click == SlotClick::Primary {
                    let _ = recipe.craft(inventory, registry);
                }
                return true;
            }
        }
        let visible_slots = if self.open {
            inventory.slots().len()
        } else {
            HOTBAR_SLOTS
        };
        let clicked = (0..visible_slots).find(|&slot| layout.contains(slot, cursor));
        if let Some(slot) = clicked {
            if slot < HOTBAR_SLOTS {
                inventory.select_hotbar(slot);
            }
            if self.open {
                inventory.click_slot(slot, &mut self.cursor_stack, click, registry);
            }
            return true;
        }
        if self.open_container.is_some()
            && let Some(container) = container.as_mut()
            && let Some(slot) =
                (0..container.slots().len()).find(|&slot| layout.container_contains(slot, cursor))
        {
            if self.processor_slots
                && self.cursor_stack.is_some_and(|stack| {
                    slot >= 2 || !processor_accepts_manual_input(slot, stack.item())
                })
            {
                return true;
            }
            container.click_slot(slot, &mut self.cursor_stack, click, registry);
            return true;
        }
        self.open
    }

    pub fn queue(
        &self,
        renderer: &mut GuiRenderer,
        inventory: &Inventory,
        furniture: Option<FurnitureGuiState<'_>>,
        registry: &ItemRegistry,
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) {
        let container = furniture.and_then(|state| state.container);
        let personal_inventory = self.open && self.open_container.is_none();
        let panel_height = if personal_inventory {
            self.page.panel_height()
        } else if self.open {
            ITEM_DETAILS_HEIGHT
        } else {
            0.0
        };
        let layout = SlotLayout::with_panel_height(
            viewport,
            container.map(|container| container.slots().len()),
            self.controls,
            panel_height,
        );
        let visible_slots = if self.open {
            inventory.slots().len()
        } else {
            HOTBAR_SLOTS
        };
        for slot in 0..visible_slots {
            let position = layout.position(slot);
            let hovered = layout.contains(slot, cursor);
            let selected = slot == inventory.selected_hotbar();
            let tint = if selected {
                [1.0, 0.72, 0.24, 1.0]
            } else if hovered {
                [1.2, 1.2, 1.2, 1.0]
            } else {
                [1.0; 4]
            };
            renderer.queue_slot(position, layout.slot_size, tint);
            if let Some(stack) = inventory.slot(slot) {
                queue_stack(renderer, registry, stack, position, layout.slot_size);
            }
        }

        if self.open_container.is_some()
            && let Some(container) = container
        {
            for slot in 0..container.slots().len() {
                let position = layout.container_position(slot);
                let tint = if layout.container_contains(slot, cursor) {
                    [1.2, 1.2, 1.2, 1.0]
                } else {
                    [1.0; 4]
                };
                renderer.queue_slot(position, layout.slot_size, tint);
                if let Some(stack) = container.slot(slot) {
                    queue_stack(renderer, registry, stack, position, layout.slot_size);
                }
                if self.processor_slots && slot < 3 {
                    let label = ["INPUT A", "INPUT B", "OUTPUT"][slot];
                    renderer.queue_text(
                        label,
                        [
                            position[0] - GuiRenderer::text_width(label, 0.75) * 0.5,
                            position[1] - layout.slot_size * 0.5 - 11.0,
                        ],
                        0.75,
                        if slot == 2 {
                            [0.95, 0.72, 0.28, 1.0]
                        } else {
                            [0.42, 0.86, 0.92, 1.0]
                        },
                    );
                }
            }
        }

        if self.open_container.is_some() {
            let title_position =
                layout.furniture_title_position(container_slots_from(container), self.controls);
            renderer.queue_text(
                self.open_title.unwrap_or("CONTAINER"),
                title_position,
                1.5,
                [1.0, 0.82, 0.42, 1.0],
            );
            if self.controls.activation {
                let (centre, size) =
                    layout.activation_button(container_slots_from(container), self.controls);
                let active = furniture.is_some_and(|state| state.active);
                let hovered = rect_contains(centre, size, cursor);
                let base = if active {
                    [0.48, 0.13, 0.12, 1.0]
                } else {
                    [0.08, 0.42, 0.24, 1.0]
                };
                let tint = if hovered {
                    [base[0] * 1.25, base[1] * 1.25, base[2] * 1.25, 1.0]
                } else {
                    base
                };
                renderer.queue_rect(centre, size, tint);
                let label = if active { "DEACTIVATE" } else { "ACTIVATE" };
                renderer.queue_text(
                    label,
                    [
                        centre[0] - GuiRenderer::text_width(label, 2.0) * 0.5,
                        centre[1] - 7.0,
                    ],
                    2.0,
                    [1.0; 4],
                );
            }
            if self.controls.target_priority {
                let selected = furniture.and_then(|state| state.target_priority);
                let buttons = layout.target_buttons(container_slots_from(container), self.controls);
                renderer.queue_text(
                    "TARGET PRIORITY",
                    [
                        buttons[0].0[0] - buttons[0].1[0] * 0.5,
                        buttons[0].0[1] - 27.0,
                    ],
                    1.5,
                    [0.62, 0.78, 0.92, 1.0],
                );
                for (priority, (centre, size)) in TargetPriority::ALL.into_iter().zip(buttons) {
                    let unlocked = self.advanced_targeting || priority == TargetPriority::Closest;
                    let hovered = unlocked && rect_contains(centre, size, cursor);
                    let base = if selected == Some(priority) {
                        [0.12, 0.42, 0.34, 1.0]
                    } else if !unlocked {
                        [0.08, 0.09, 0.11, 1.0]
                    } else {
                        [0.08, 0.16, 0.28, 1.0]
                    };
                    let tint = if hovered {
                        [base[0] * 1.25, base[1] * 1.25, base[2] * 1.25, 1.0]
                    } else {
                        base
                    };
                    renderer.queue_rect(centre, size, tint);
                    renderer.queue_text(
                        priority.label(),
                        [
                            centre[0] - GuiRenderer::text_width(priority.label(), 1.0) * 0.5,
                            centre[1] - 3.5,
                        ],
                        1.0,
                        if unlocked {
                            [1.0; 4]
                        } else {
                            [0.35, 0.38, 0.42, 1.0]
                        },
                    );
                }
            }
            if self.controls.laser_aim {
                let selected = furniture.and_then(|state| state.laser_aim);
                let buttons =
                    layout.laser_aim_buttons(container_slots_from(container), self.controls);
                renderer.queue_text(
                    "DRILL AIM",
                    [
                        buttons[0].0[0] - buttons[0].1[0] * 0.5,
                        buttons[0].0[1] - 27.0,
                    ],
                    1.5,
                    [0.62, 0.84, 0.94, 1.0],
                );
                for (aim, button) in LaserDrillAim::ALL.into_iter().zip(buttons) {
                    queue_control_button(
                        renderer,
                        button,
                        aim.label(),
                        selected == Some(aim),
                        cursor,
                    );
                }
            }
            if self.controls.lift_controls {
                let lift_status = furniture
                    .and_then(|state| state.lift_status)
                    .unwrap_or(CargoLiftStatus::new(CargoLiftDirection::Idle, false));
                let direction = lift_status.direction;
                let powered = lift_status.powered;
                let buttons = layout.lift_buttons(container_slots_from(container), self.controls);
                let status = if powered {
                    direction.label()
                } else {
                    "NO POWER"
                };
                renderer.queue_text(
                    status,
                    [
                        buttons[0].0[0] - buttons[0].1[0] * 0.5,
                        buttons[0].0[1] - 25.0,
                    ],
                    1.5,
                    if powered {
                        [0.45, 0.9, 0.78, 1.0]
                    } else {
                        [0.9, 0.35, 0.28, 1.0]
                    },
                );
                for (button, label, selected) in [
                    (buttons[0], "UP", direction == CargoLiftDirection::Up),
                    (buttons[1], "DOWN", direction == CargoLiftDirection::Down),
                ] {
                    let hovered = rect_contains(button.0, button.1, cursor);
                    let base: [f32; 4] = if selected {
                        [0.10, 0.44, 0.38, 1.0]
                    } else {
                        [0.08, 0.18, 0.25, 1.0]
                    };
                    let tint = if hovered {
                        [base[0] * 1.25, base[1] * 1.25, base[2] * 1.25, 1.0]
                    } else {
                        base
                    };
                    renderer.queue_rect(button.0, button.1, tint);
                    renderer.queue_text(
                        label,
                        [
                            button.0[0] - GuiRenderer::text_width(label, 2.0) * 0.5,
                            button.0[1] - 7.0,
                        ],
                        2.0,
                        [1.0; 4],
                    );
                }
            }
            if self.controls.lift_station_controls {
                let configuration = furniture
                    .and_then(|state| state.lift_station_configuration)
                    .unwrap_or_default();
                let [modes, departures] =
                    layout.lift_station_buttons(container_slots_from(container), self.controls);
                for (buttons, heading) in [(modes, "TRANSFER"), (departures, "THEN SEND")] {
                    renderer.queue_text(
                        heading,
                        [
                            buttons[0].0[0] - buttons[0].1[0] * 0.5,
                            buttons[0].0[1] - 25.0,
                        ],
                        1.5,
                        [0.62, 0.84, 0.94, 1.0],
                    );
                }
                for (mode, button) in LiftStationMode::ALL.into_iter().zip(modes) {
                    queue_control_button(
                        renderer,
                        button,
                        mode.label(),
                        configuration.mode() == mode,
                        cursor,
                    );
                }
                for (direction, label, button) in [
                    (CargoLiftDirection::Up, "UP", departures[0]),
                    (CargoLiftDirection::Down, "DOWN", departures[1]),
                ] {
                    queue_control_button(
                        renderer,
                        button,
                        label,
                        configuration.departure() == direction,
                        cursor,
                    );
                }
            }
            if self.controls.battery_status
                && let Some(status) = furniture.and_then(|state| state.battery_status)
            {
                let (centre, size) =
                    layout.battery_meter(container_slots_from(container), self.controls);
                renderer.queue_text(
                    "STORED POWER",
                    [centre[0] - size[0] * 0.5, centre[1] - 24.0],
                    1.5,
                    [0.62, 0.84, 0.94, 1.0],
                );
                renderer.queue_rect(centre, size, [0.055, 0.075, 0.11, 1.0]);
                let fill_width = size[0] * status.fraction();
                if fill_width > 0.0 {
                    renderer.queue_rect(
                        [centre[0] - size[0] * 0.5 + fill_width * 0.5, centre[1]],
                        [fill_width, size[1]],
                        [0.0, 0.58, 0.72, 1.0],
                    );
                }
                let value = format!("{}/{}", status.stored_milli, status.capacity_milli);
                renderer.queue_text(
                    &value,
                    [
                        centre[0] - GuiRenderer::text_width(&value, 1.0) * 0.5,
                        centre[1] - 3.5,
                    ],
                    1.0,
                    [1.0; 4],
                );
            }
            let mut status_index = 0;
            if self.controls.drill_depth_status {
                let value = furniture
                    .and_then(|state| state.drill_depth_decimetres)
                    .map_or_else(
                        || "DRILL DEPTH IDLE".to_owned(),
                        |depth| format!("DRILL DEPTH {}", signed_metres(depth)),
                    );
                let centre = layout.status_line(
                    container_slots_from(container),
                    self.controls,
                    status_index,
                );
                renderer.queue_text(
                    &value,
                    [
                        centre[0] - GuiRenderer::text_width(&value, 1.5) * 0.5,
                        centre[1],
                    ],
                    1.5,
                    [0.62, 0.84, 0.94, 1.0],
                );
                status_index += 1;
            }
            if self.controls.kill_count_status {
                let value = format!(
                    "KILLS {}",
                    furniture
                        .and_then(|state| state.turret_kill_count)
                        .unwrap_or(0)
                );
                let centre = layout.status_line(
                    container_slots_from(container),
                    self.controls,
                    status_index,
                );
                renderer.queue_text(
                    &value,
                    [
                        centre[0] - GuiRenderer::text_width(&value, 1.5) * 0.5,
                        centre[1],
                    ],
                    1.5,
                    [0.82, 0.68, 0.42, 1.0],
                );
                status_index += 1;
            }
            if self.controls.subsurface_survey_status {
                let state = furniture.and_then(|state| state.subsurface_survey);
                let active = furniture.is_some_and(|state| state.active);
                let mut lines = Vec::new();
                let colour = if !active {
                    lines.push("SURVEY STANDBY - ACTIVATE TO SCAN".to_owned());
                    [0.62, 0.68, 0.72, 1.0]
                } else if !state.is_some_and(|state| state.powered) {
                    lines.push("SURVEY OFFLINE - NO POWER".to_owned());
                    [0.9, 0.35, 0.28, 1.0]
                } else if let Some(survey) = state.and_then(|state| state.survey) {
                    let depth_metres = survey.depth_tiles as f32 * crate::METRES_PER_TILE;
                    lines.push(format!(
                        "SCAN {} TILES WIDE / {:.0} M DEEP",
                        survey.width_tiles, depth_metres
                    ));
                    for estimate in survey.estimates() {
                        let name = registry
                            .get(estimate.item)
                            .map_or("UNKNOWN ORE", |definition| definition.name.as_str());
                        lines.push(format!(
                            "{name}: ~{} YIELD / START {:+.1}M",
                            estimate.estimated_yield,
                            estimate.first_depth_decimetres as f32 / 10.0
                        ));
                    }
                    if lines.len() == 1 {
                        lines.push("NO REGISTERED ORE DETECTED".to_owned());
                    }
                    [0.45, 0.9, 0.78, 1.0]
                } else {
                    lines.push("SURVEY CALIBRATING".to_owned());
                    [0.82, 0.68, 0.42, 1.0]
                };
                for (offset, value) in lines.iter().take(3).enumerate() {
                    let centre = layout.status_line(
                        container_slots_from(container),
                        self.controls,
                        status_index + offset,
                    );
                    renderer.queue_text(
                        value,
                        [
                            centre[0] - GuiRenderer::text_width(value, 1.25) * 0.5,
                            centre[1],
                        ],
                        1.25,
                        colour,
                    );
                }
            }
        }

        if personal_inventory {
            let crafting_layout = CraftingLayout::new(layout, panel_height);
            match self.page {
                InventoryPage::Items => queue_personal_item_panel(
                    renderer,
                    registry,
                    self.cursor_stack.or_else(|| inventory.selected_stack()),
                    crafting_layout,
                    cursor,
                ),
                InventoryPage::Crafting => {
                    queue_crafting_panel(
                        renderer,
                        inventory,
                        registry,
                        crafting_layout,
                        self.crafting_offset,
                        self.craftable_only,
                        cursor,
                    );
                }
            }
        } else if self.open {
            queue_item_details(
                renderer,
                registry,
                self.cursor_stack.or_else(|| inventory.selected_stack()),
                layout.item_details_panel(container_slots_from(container), self.controls),
            );
        }

        if self.open
            && let Some(stack) = self.cursor_stack
        {
            queue_stack(renderer, registry, stack, cursor, layout.slot_size);
        }
    }
}
