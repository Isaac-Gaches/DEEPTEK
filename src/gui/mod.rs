mod contracts;
mod font;
mod hud;
mod map;
mod renderer;

pub use contracts::{ContractsAction, ContractsGui};
pub use hud::{HudAction, HudGui, HudSnapshot, MeterValue};
pub use map::WorldMapGui;
pub use renderer::GuiRenderer;

use crate::{
    CHEST_COLUMNS, CargoLiftDirection, ConsumableAction, FurnitureConfiguration,
    FurnitureInteraction, HOTBAR_SLOTS, INVENTORY_COLUMNS, Inventory, ItemAction, ItemCategory,
    ItemContainer, ItemRegistry, ItemStack, Layer, LiftStationConfiguration, LiftStationMode,
    ObjectId, ProjectileKind, SlotClick, TargetPriority, ToolAction,
};

const MAX_SLOT_SIZE: f32 = 48.0;
const SLOT_GAP: f32 = 4.0;
const SCREEN_MARGIN: f32 = 14.0;
const ITEM_DETAILS_HEIGHT: f32 = 96.0;
const ITEM_DETAILS_GAP: f32 = 10.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FurnitureControls {
    activation: bool,
    target_priority: bool,
    battery_status: bool,
    drill_depth_status: bool,
    kill_count_status: bool,
    lift_controls: bool,
    lift_station_controls: bool,
}

impl FurnitureControls {
    const fn from_interaction(interaction: FurnitureInteraction) -> Self {
        Self {
            activation: interaction.is_activatable(),
            target_priority: matches!(
                interaction.configuration(),
                Some(FurnitureConfiguration::TargetPriority)
            ),
            battery_status: interaction.shows_power_storage(),
            drill_depth_status: interaction.shows_drill_depth(),
            kill_count_status: interaction.shows_kill_count(),
            lift_controls: interaction.shows_lift_controls(),
            lift_station_controls: interaction.shows_lift_station_controls(),
        }
    }

    const fn height(self) -> f32 {
        (if self.activation { 46.0 } else { 0.0 })
            + (if self.target_priority { 72.0 } else { 0.0 })
            + (if self.battery_status { 52.0 } else { 0.0 })
            + (if self.drill_depth_status { 26.0 } else { 0.0 })
            + (if self.kill_count_status { 26.0 } else { 0.0 })
            + (if self.lift_controls { 70.0 } else { 0.0 })
            + (if self.lift_station_controls {
                116.0
            } else {
                0.0
            })
    }
}

#[derive(Clone, Copy)]
struct SlotLayout {
    first_centre: [f32; 2],
    slot_size: f32,
    stride: f32,
}

impl SlotLayout {
    fn new(viewport: [f32; 2]) -> Self {
        Self::with_container(viewport, None)
    }

    fn with_container(viewport: [f32; 2], container_slots: Option<usize>) -> Self {
        Self::with_furniture(viewport, container_slots, FurnitureControls::default())
    }

    fn with_furniture(
        viewport: [f32; 2],
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> Self {
        Self::with_details(viewport, container_slots, controls, false)
    }

    fn with_details(
        viewport: [f32; 2],
        container_slots: Option<usize>,
        controls: FurnitureControls,
        show_details: bool,
    ) -> Self {
        let available = (viewport[0] - SCREEN_MARGIN * 2.0).max(200.0);
        let width_size = (available - SLOT_GAP * (HOTBAR_SLOTS - 1) as f32) / HOTBAR_SLOTS as f32;
        let height_size = if container_slots.is_some() || controls.height() > 0.0 || show_details {
            let container_rows = container_slots
                .map(|slots| slots.div_ceil(CHEST_COLUMNS).max(1))
                .unwrap_or(0);
            let rows = crate::INVENTORY_ROWS + container_rows;
            let furniture_header = if container_slots.is_some() || controls.height() > 0.0 {
                28.0
            } else {
                0.0
            };
            let container_separator = if container_slots.is_some() { 20.0 } else { 0.0 };
            let details_height = if show_details {
                ITEM_DETAILS_HEIGHT + ITEM_DETAILS_GAP
            } else {
                0.0
            };
            (viewport[1]
                - SCREEN_MARGIN * 2.0
                - furniture_header
                - container_separator
                - controls.height()
                - details_height
                - SLOT_GAP * (rows - 1) as f32)
                / rows as f32
        } else {
            MAX_SLOT_SIZE
        };
        let slot_size = width_size.min(height_size).clamp(14.0, MAX_SLOT_SIZE);
        let stride = slot_size + SLOT_GAP;
        let total_width =
            slot_size * HOTBAR_SLOTS as f32 + SLOT_GAP * (HOTBAR_SLOTS.saturating_sub(1)) as f32;
        Self {
            first_centre: [
                (viewport[0] - total_width) * 0.5 + slot_size * 0.5,
                viewport[1] - SCREEN_MARGIN - slot_size * 0.5,
            ],
            slot_size,
            stride,
        }
    }

    fn position(self, slot: usize) -> [f32; 2] {
        let column = slot % INVENTORY_COLUMNS;
        let row_from_hotbar = if slot < HOTBAR_SLOTS {
            0
        } else {
            (slot - HOTBAR_SLOTS) / INVENTORY_COLUMNS + 1
        };
        [
            self.first_centre[0] + column as f32 * self.stride,
            self.first_centre[1] - row_from_hotbar as f32 * self.stride,
        ]
    }

    fn contains(self, slot: usize, cursor: [f32; 2]) -> bool {
        let position = self.position(slot);
        let half = self.slot_size * 0.5;
        (position[0] - half..=position[0] + half).contains(&cursor[0])
            && (position[1] - half..=position[1] + half).contains(&cursor[1])
    }

    fn container_position(self, slot: usize) -> [f32; 2] {
        let column = slot % CHEST_COLUMNS;
        let row_from_bottom = slot / CHEST_COLUMNS;
        [
            self.first_centre[0] + column as f32 * self.stride,
            self.first_centre[1]
                - (crate::INVENTORY_ROWS + row_from_bottom) as f32 * self.stride
                - 20.0,
        ]
    }

    fn container_contains(self, slot: usize, cursor: [f32; 2]) -> bool {
        let position = self.container_position(slot);
        let half = self.slot_size * 0.5;
        (position[0] - half..=position[0] + half).contains(&cursor[0])
            && (position[1] - half..=position[1] + half).contains(&cursor[1])
    }

    fn furniture_top(self, container_slots: Option<usize>) -> f32 {
        let top_slot = container_slots.map_or_else(
            || self.position((crate::INVENTORY_ROWS - 1) * INVENTORY_COLUMNS),
            |slots| {
                let rows = slots.div_ceil(CHEST_COLUMNS).max(1);
                self.container_position((rows - 1) * CHEST_COLUMNS)
            },
        );
        top_slot[1] - self.slot_size * 0.5
    }

    fn control_width(self) -> f32 {
        (self.stride * CHEST_COLUMNS as f32 - SLOT_GAP).min(520.0)
    }

    fn activation_button(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> ([f32; 2], [f32; 2]) {
        debug_assert!(controls.activation);
        (
            [
                self.first_centre[0] + self.stride * (CHEST_COLUMNS as f32 - 1.0) * 0.5,
                self.furniture_top(container_slots) - 22.0,
            ],
            [self.control_width().min(240.0), 34.0],
        )
    }

    fn target_buttons(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> [([f32; 2], [f32; 2]); 4] {
        debug_assert!(controls.target_priority);
        let total_width = self.control_width();
        let gap = 4.0;
        let width = (total_width - gap * 3.0) / 4.0;
        let centre_x = self.first_centre[0] + self.stride * (CHEST_COLUMNS as f32 - 1.0) * 0.5;
        let offset = if controls.activation { 46.0 } else { 0.0 };
        let centre_y = self.furniture_top(container_slots) - offset - 35.0;
        std::array::from_fn(|index| {
            (
                [
                    centre_x - total_width * 0.5 + width * 0.5 + index as f32 * (width + gap),
                    centre_y,
                ],
                [width, 32.0],
            )
        })
    }

    fn furniture_title_position(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> [f32; 2] {
        [
            self.first_centre[0],
            self.furniture_top(container_slots) - controls.height() - 16.0,
        ]
    }

    fn item_details_panel(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> ([f32; 2], [f32; 2]) {
        let grid_centre_x = self.first_centre[0] + self.stride * (HOTBAR_SLOTS as f32 - 1.0) * 0.5;
        let grid_width = self.stride * HOTBAR_SLOTS as f32 - SLOT_GAP;
        let content_top = if container_slots.is_some() || controls.height() > 0.0 {
            self.furniture_top(container_slots) - controls.height() - 28.0
        } else {
            let top_row = (crate::INVENTORY_ROWS - 1) * INVENTORY_COLUMNS;
            self.position(top_row)[1] - self.slot_size * 0.5
        };
        (
            [
                grid_centre_x,
                content_top - ITEM_DETAILS_GAP - ITEM_DETAILS_HEIGHT * 0.5,
            ],
            [grid_width.min(360.0), ITEM_DETAILS_HEIGHT],
        )
    }

    fn battery_meter(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> ([f32; 2], [f32; 2]) {
        debug_assert!(controls.battery_status);
        let preceding_height = (if controls.activation { 46.0 } else { 0.0 })
            + (if controls.target_priority { 72.0 } else { 0.0 });
        (
            [
                self.first_centre[0] + self.stride * (CHEST_COLUMNS as f32 - 1.0) * 0.5,
                self.furniture_top(container_slots) - preceding_height - 20.0,
            ],
            [self.control_width(), 18.0],
        )
    }

    fn status_line(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
        index: usize,
    ) -> [f32; 2] {
        let preceding_height = (if controls.activation { 46.0 } else { 0.0 })
            + (if controls.target_priority { 72.0 } else { 0.0 })
            + (if controls.battery_status { 52.0 } else { 0.0 });
        [
            self.first_centre[0] + self.stride * (CHEST_COLUMNS as f32 - 1.0) * 0.5,
            self.furniture_top(container_slots) - preceding_height - 18.0 - index as f32 * 26.0,
        ]
    }

    fn lift_buttons(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> [([f32; 2], [f32; 2]); 2] {
        debug_assert!(controls.lift_controls);
        let width = (self.control_width() - SLOT_GAP) * 0.5;
        let centre_x = self.first_centre[0] + self.stride * (CHEST_COLUMNS as f32 - 1.0) * 0.5;
        let preceding = (if controls.activation { 46.0 } else { 0.0 })
            + (if controls.target_priority { 72.0 } else { 0.0 })
            + (if controls.battery_status { 52.0 } else { 0.0 })
            + (if controls.drill_depth_status {
                26.0
            } else {
                0.0
            })
            + (if controls.kill_count_status {
                26.0
            } else {
                0.0
            });
        let centre_y = self.furniture_top(container_slots) - preceding - 24.0;
        [
            (
                [centre_x - (width + SLOT_GAP) * 0.5, centre_y],
                [width, 34.0],
            ),
            (
                [centre_x + (width + SLOT_GAP) * 0.5, centre_y],
                [width, 34.0],
            ),
        ]
    }

    fn lift_station_buttons(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> [[([f32; 2], [f32; 2]); 2]; 2] {
        debug_assert!(controls.lift_station_controls);
        let width = (self.control_width() - SLOT_GAP) * 0.5;
        let centre_x = self.first_centre[0] + self.stride * (CHEST_COLUMNS as f32 - 1.0) * 0.5;
        let row = |centre_y: f32| {
            [
                (
                    [centre_x - (width + SLOT_GAP) * 0.5, centre_y],
                    [width, 34.0],
                ),
                (
                    [centre_x + (width + SLOT_GAP) * 0.5, centre_y],
                    [width, 34.0],
                ),
            ]
        };
        let top = self.furniture_top(container_slots);
        [row(top - 24.0), row(top - 82.0)]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InventoryGui {
    open: bool,
    open_container: Option<ObjectId>,
    open_title: Option<&'static str>,
    controls: FurnitureControls,
    cursor_stack: Option<ItemStack>,
}

#[derive(Clone, Copy)]
pub struct FurnitureGuiState<'a> {
    container: Option<&'a ItemContainer>,
    active: bool,
    target_priority: Option<TargetPriority>,
    battery_status: Option<BatteryStatus>,
    drill_depth_decimetres: Option<i32>,
    turret_kill_count: Option<u32>,
    lift_status: Option<CargoLiftStatus>,
    lift_station_configuration: Option<LiftStationConfiguration>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CargoLiftStatus {
    pub direction: CargoLiftDirection,
    pub powered: bool,
}

impl CargoLiftStatus {
    pub const fn new(direction: CargoLiftDirection, powered: bool) -> Self {
        Self { direction, powered }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryStatus {
    pub stored_milli: u32,
    pub capacity_milli: u32,
}

impl BatteryStatus {
    pub const fn new(stored_milli: u32, capacity_milli: u32) -> Self {
        Self {
            stored_milli,
            capacity_milli,
        }
    }

    fn fraction(self) -> f32 {
        if self.capacity_milli == 0 {
            0.0
        } else {
            self.stored_milli.min(self.capacity_milli) as f32 / self.capacity_milli as f32
        }
    }
}

impl<'a> FurnitureGuiState<'a> {
    pub const fn new(container: Option<&'a ItemContainer>, active: bool) -> Self {
        Self {
            container,
            active,
            target_priority: None,
            battery_status: None,
            drill_depth_decimetres: None,
            turret_kill_count: None,
            lift_status: None,
            lift_station_configuration: None,
        }
    }

    pub const fn with_target_priority(mut self, value: Option<TargetPriority>) -> Self {
        self.target_priority = value;
        self
    }

    pub const fn with_battery_status(mut self, value: Option<BatteryStatus>) -> Self {
        self.battery_status = value;
        self
    }

    pub const fn with_drill_depth(mut self, value: Option<i32>) -> Self {
        self.drill_depth_decimetres = value;
        self
    }

    pub const fn with_turret_kill_count(mut self, value: Option<u32>) -> Self {
        self.turret_kill_count = value;
        self
    }

    pub const fn with_lift_status(mut self, value: Option<CargoLiftStatus>) -> Self {
        self.lift_status = value;
        self
    }

    pub const fn with_lift_station_configuration(
        mut self,
        value: Option<LiftStationConfiguration>,
    ) -> Self {
        self.lift_station_configuration = value;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FurnitureControlAction {
    SetActive(ObjectId, bool),
    SetTargetPriority(ObjectId, TargetPriority),
    MoveCargoLift(ObjectId, CargoLiftDirection),
    SetLiftStationMode(ObjectId, LiftStationMode),
    SetLiftStationDeparture(ObjectId, CargoLiftDirection),
}

impl InventoryGui {
    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub const fn cursor_stack(&self) -> Option<ItemStack> {
        self.cursor_stack
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
    }

    pub fn show_container(&mut self, object: ObjectId) {
        self.open = true;
        self.open_container = Some(object);
        self.open_title = Some("CONTAINER");
        self.controls = FurnitureControls::default();
    }

    pub fn show_furniture(
        &mut self,
        object: ObjectId,
        title: &'static str,
        interaction: FurnitureInteraction,
    ) {
        self.open = true;
        self.open_container = Some(object);
        self.open_title = Some(title);
        self.controls = FurnitureControls::from_interaction(interaction);
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
                if rect_contains(centre, size, cursor) {
                    return Some(FurnitureControlAction::SetTargetPriority(object, priority));
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
        let layout = SlotLayout::with_details(viewport, container_slots, self.controls, self.open);
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
        let layout = SlotLayout::with_details(
            viewport,
            container.map(|container| container.slots().len()),
            self.controls,
            self.open,
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
                    let hovered = rect_contains(centre, size, cursor);
                    let base = if selected == Some(priority) {
                        [0.12, 0.42, 0.34, 1.0]
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
                        [1.0; 4],
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
            }
        }

        if self.open {
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

fn signed_metres(decimetres: i32) -> String {
    let sign = if decimetres >= 0 { '+' } else { '-' };
    let magnitude = decimetres.unsigned_abs();
    format!("{sign}{}.{:01}M", magnitude / 10, magnitude % 10)
}

fn container_slots_from(container: Option<&ItemContainer>) -> Option<usize> {
    container.map(|container| container.slots().len())
}

fn queue_control_button(
    renderer: &mut GuiRenderer,
    button: ([f32; 2], [f32; 2]),
    label: &str,
    selected: bool,
    cursor: [f32; 2],
) {
    let base: [f32; 4] = if selected {
        [0.10, 0.44, 0.38, 1.0]
    } else {
        [0.08, 0.18, 0.25, 1.0]
    };
    let tint = if rect_contains(button.0, button.1, cursor) {
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

fn rect_contains(centre: [f32; 2], size: [f32; 2], point: [f32; 2]) -> bool {
    let half = [size[0] * 0.5, size[1] * 0.5];
    (centre[0] - half[0]..=centre[0] + half[0]).contains(&point[0])
        && (centre[1] - half[1]..=centre[1] + half[1]).contains(&point[1])
}

fn queue_stack(
    renderer: &mut GuiRenderer,
    registry: &ItemRegistry,
    stack: ItemStack,
    position: [f32; 2],
    slot_size: f32,
) {
    let Some(definition) = registry.get(stack.item()) else {
        return;
    };
    renderer.queue_icon(definition.icon, position, slot_size * 0.67, [1.0; 4]);
    if stack.quantity() > 1 {
        queue_quantity(renderer, stack.quantity(), position, slot_size);
    }
}

fn queue_quantity(renderer: &mut GuiRenderer, quantity: u16, position: [f32; 2], slot_size: f32) {
    let text = quantity.to_string();
    let width = GuiRenderer::text_width(&text, 1.5);
    let right = position[0] + slot_size * 0.40;
    let top_left = [right - width, position[1] + slot_size * 0.14];
    renderer.queue_text(
        &text,
        [top_left[0] + 1.0, top_left[1] + 1.0],
        1.5,
        [0.0, 0.0, 0.0, 0.9],
    );
    renderer.queue_text(&text, top_left, 1.5, [1.0; 4]);
}

fn queue_item_details(
    renderer: &mut GuiRenderer,
    registry: &ItemRegistry,
    selected: Option<ItemStack>,
    panel: ([f32; 2], [f32; 2]),
) {
    renderer.queue_rect(panel.0, panel.1, [0.025, 0.055, 0.085, 0.96]);
    let left = panel.0[0] - panel.1[0] * 0.5 + 12.0;
    let top = panel.0[1] - panel.1[1] * 0.5 + 10.0;
    let text_width = panel.1[0] - 24.0;
    let Some(definition) = selected.and_then(|stack| registry.get(stack.item())) else {
        renderer.queue_text(
            "NO ITEM SELECTED",
            [left, top],
            1.5,
            [0.62, 0.84, 0.94, 1.0],
        );
        return;
    };

    let name = fit_text(&definition.name.to_uppercase(), 1.5, text_width);
    renderer.queue_text(&name, [left, top], 1.5, [1.0, 0.78, 0.25, 1.0]);
    renderer.queue_text(
        &format!("SELL VALUE {}", definition.export_value),
        [left, top + 24.0],
        1.0,
        [0.58, 1.0, 0.68, 1.0],
    );
    renderer.queue_text(
        &format!("TYPE {}", category_label(definition.category)),
        [left, top + 42.0],
        1.0,
        [0.62, 0.84, 0.94, 1.0],
    );
    if let Some(function) = item_function(definition.action) {
        let function = fit_text(&format!("FUNCTION {function}"), 1.0, text_width);
        renderer.queue_text(&function, [left, top + 60.0], 1.0, [0.88, 0.91, 0.96, 1.0]);
    }
}

const fn category_label(category: ItemCategory) -> &'static str {
    match category {
        ItemCategory::Block => "BLOCK",
        ItemCategory::Tool => "TOOL",
        ItemCategory::Consumable => "CONSUMABLE",
        ItemCategory::Furniture => "MACHINE",
        ItemCategory::Material => "MATERIAL",
        ItemCategory::Custom => "CUSTOM",
    }
}

fn item_function(action: ItemAction) -> Option<String> {
    match action {
        ItemAction::None => None,
        ItemAction::PlaceTile { layer, .. } => Some(match layer {
            Layer::Foreground => "PLACE FOREGROUND BLOCK".to_owned(),
            Layer::Background => "PLACE BACKGROUND BLOCK".to_owned(),
        }),
        ItemAction::PlaceFurniture { .. } => Some("PLACE MACHINE".to_owned()),
        ItemAction::PlaceRope => Some("PLACE ROPE".to_owned()),
        ItemAction::PlacePoweredCable => Some("PLACE POWER CABLE".to_owned()),
        ItemAction::PlaceCargoLift => Some("PLACE CARGO LIFT".to_owned()),
        ItemAction::Tool(ToolAction::RemoveTile { layer, power }) => Some(format!(
            "MINE {} BLOCKS (POWER {power})",
            match layer {
                Layer::Foreground => "FOREGROUND",
                Layer::Background => "BACKGROUND",
            }
        )),
        ItemAction::Consume(ConsumableAction::Heal { amount }) => {
            Some(format!("RESTORE {amount} HEALTH"))
        }
        ItemAction::Throw(ProjectileKind::GlowStick) => Some("THROW LIGHT SOURCE".to_owned()),
        ItemAction::Throw(ProjectileKind::Bomb) => Some("THROW EXPLOSIVE".to_owned()),
        ItemAction::Custom(id) => Some(format!("CUSTOM ACTION {id}")),
    }
}

fn fit_text(text: &str, scale: f32, maximum_width: f32) -> String {
    if GuiRenderer::text_width(text, scale) <= maximum_width {
        return text.to_owned();
    }
    let ellipsis = "...";
    let mut fitted = String::new();
    for character in text.chars() {
        let mut candidate = fitted.clone();
        candidate.push(character);
        candidate.push_str(ellipsis);
        if GuiRenderer::text_width(&candidate, scale) > maximum_width {
            break;
        }
        fitted.push(character);
    }
    fitted.push_str(ellipsis);
    fitted
}

#[cfg(test)]
mod tests {
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
            Some("MINE FOREGROUND BLOCKS (POWER 1)")
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
        let mut inventory = Inventory::starter(&registry);
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
        let mut inventory = Inventory::starter(&registry);
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
    fn inventory_rows_are_arranged_above_the_hotbar() {
        let layout = SlotLayout::new([800.0, 600.0]);
        assert!(layout.position(HOTBAR_SLOTS)[1] < layout.position(0)[1]);
        assert!(
            layout.position(HOTBAR_SLOTS * crate::INVENTORY_ROWS - 1)[1] < layout.position(0)[1]
        );
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
        let mut inventory = Inventory::starter(&registry);
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
        for x in [2, 4] {
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
            battery_status: false,
            drill_depth_status: true,
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
    fn targeting_controls_work_without_container_storage() {
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
            Some(FurnitureControlAction::SetTargetPriority(
                turret,
                TargetPriority::Strongest
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
        let control_centre = layout.target_buttons(None, turret)[0].0[0]
            + layout.target_buttons(None, turret)[3].0[0];
        assert!((status[0] - control_centre * 0.5).abs() < f32::EPSILON);
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
}
