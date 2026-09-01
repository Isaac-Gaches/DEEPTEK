mod view;

use super::GuiRenderer;
use crate::{
    CHEST_COLUMNS, CRAFTING_RECIPES, CargoLiftDirection, ConsumableAction, CraftingError,
    CraftingRecipe, FurnitureConfiguration, FurnitureInteraction, HOTBAR_SLOTS, INVENTORY_COLUMNS,
    Inventory, ItemAction, ItemCategory, ItemContainer, ItemRegistry, ItemStack, ItemTransportRole,
    LaserDrillAim, Layer, LiftStationConfiguration, LiftStationMode, ObjectId, ProjectileKind,
    SlotClick, SubsurfaceSurvey, TargetPriority, ToolAction, processor_accepts_manual_input,
};

const MAX_SLOT_SIZE: f32 = 48.0;
const SLOT_GAP: f32 = 4.0;
const SCREEN_MARGIN: f32 = 14.0;
const ITEM_DETAILS_HEIGHT: f32 = 96.0;
const ITEM_DETAILS_GAP: f32 = 10.0;
const PERSONAL_ITEM_PANEL_HEIGHT: f32 = 136.0;
const CRAFTING_PANEL_HEIGHT: f32 = 276.0;
const INVENTORY_TAB_HEIGHT: f32 = 28.0;
const CRAFTING_COLUMNS: usize = 2;
const CRAFTING_VISIBLE_ROWS: usize = 3;
const CRAFTING_VISIBLE_RECIPES: usize = CRAFTING_COLUMNS * CRAFTING_VISIBLE_ROWS;
const CRAFTING_SCROLL_FOOTER_HEIGHT: f32 = 18.0;
const CRAFTING_FILTER_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FurnitureControls {
    activation: bool,
    target_priority: bool,
    laser_aim: bool,
    battery_status: bool,
    drill_depth_status: bool,
    subsurface_survey_status: bool,
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
            laser_aim: matches!(
                interaction.configuration(),
                Some(FurnitureConfiguration::LaserAim)
            ),
            battery_status: interaction.shows_power_storage(),
            drill_depth_status: interaction.shows_drill_depth(),
            subsurface_survey_status: interaction.shows_subsurface_survey(),
            kill_count_status: interaction.shows_kill_count(),
            lift_controls: interaction.shows_lift_controls(),
            lift_station_controls: interaction.shows_lift_station_controls(),
        }
    }

    const fn height(self) -> f32 {
        (if self.activation { 46.0 } else { 0.0 })
            + (if self.target_priority { 72.0 } else { 0.0 })
            + (if self.laser_aim { 72.0 } else { 0.0 })
            + (if self.battery_status { 52.0 } else { 0.0 })
            + (if self.drill_depth_status { 26.0 } else { 0.0 })
            + (if self.subsurface_survey_status {
                78.0
            } else {
                0.0
            })
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
        Self::with_panel_height(
            viewport,
            container_slots,
            controls,
            if show_details {
                ITEM_DETAILS_HEIGHT
            } else {
                0.0
            },
        )
    }

    fn with_panel_height(
        viewport: [f32; 2],
        container_slots: Option<usize>,
        controls: FurnitureControls,
        details_height: f32,
    ) -> Self {
        let available = (viewport[0] - SCREEN_MARGIN * 2.0).max(200.0);
        let width_size = (available - SLOT_GAP * (HOTBAR_SLOTS - 1) as f32) / HOTBAR_SLOTS as f32;
        let height_size =
            if container_slots.is_some() || controls.height() > 0.0 || details_height > 0.0 {
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
                let details_height = if details_height > 0.0 {
                    details_height + ITEM_DETAILS_GAP
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

    fn laser_aim_buttons(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> [([f32; 2], [f32; 2]); 7] {
        debug_assert!(controls.laser_aim);
        let total_width = self.control_width();
        let gap = 3.0;
        let width = (total_width - gap * 6.0) / 7.0;
        let centre_x = self.first_centre[0] + self.stride * (CHEST_COLUMNS as f32 - 1.0) * 0.5;
        let preceding = (if controls.activation { 46.0 } else { 0.0 })
            + (if controls.target_priority { 72.0 } else { 0.0 });
        let centre_y = self.furniture_top(container_slots) - preceding - 35.0;
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
        self.details_panel(container_slots, controls, ITEM_DETAILS_HEIGHT, 360.0)
    }

    fn details_panel(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
        height: f32,
        maximum_width: f32,
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
            [grid_centre_x, content_top - ITEM_DETAILS_GAP - height * 0.5],
            [grid_width.min(maximum_width), height],
        )
    }

    fn battery_meter(
        self,
        container_slots: Option<usize>,
        controls: FurnitureControls,
    ) -> ([f32; 2], [f32; 2]) {
        debug_assert!(controls.battery_status);
        let preceding_height = (if controls.activation { 46.0 } else { 0.0 })
            + (if controls.target_priority { 72.0 } else { 0.0 })
            + (if controls.laser_aim { 72.0 } else { 0.0 });
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
            + (if controls.laser_aim { 72.0 } else { 0.0 })
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
            + (if controls.laser_aim { 72.0 } else { 0.0 })
            + (if controls.battery_status { 52.0 } else { 0.0 })
            + (if controls.drill_depth_status {
                26.0
            } else {
                0.0
            })
            + (if controls.subsurface_survey_status {
                78.0
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

#[derive(Clone, Copy)]
struct CraftingLayout {
    panel: ([f32; 2], [f32; 2]),
}

impl CraftingLayout {
    fn new(layout: SlotLayout, height: f32) -> Self {
        Self {
            panel: layout.details_panel(None, FurnitureControls::default(), height, 520.0),
        }
    }

    fn tab_buttons(self) -> [([f32; 2], [f32; 2]); 2] {
        let gap = 4.0;
        let width = (self.panel.1[0] - 16.0 - gap) * 0.5;
        let y = self.panel.0[1] - self.panel.1[1] * 0.5 + 8.0 + INVENTORY_TAB_HEIGHT * 0.5;
        [
            (
                [self.panel.0[0] - (width + gap) * 0.5, y],
                [width, INVENTORY_TAB_HEIGHT],
            ),
            (
                [self.panel.0[0] + (width + gap) * 0.5, y],
                [width, INVENTORY_TAB_HEIGHT],
            ),
        ]
    }

    fn recipe_button(self, index: usize) -> ([f32; 2], [f32; 2]) {
        let gap = 4.0;
        let padding = 8.0;
        let top = self.panel.0[1] - self.panel.1[1] * 0.5
            + padding
            + INVENTORY_TAB_HEIGHT
            + gap
            + CRAFTING_FILTER_HEIGHT
            + gap;
        let available_height = self.panel.1[1]
            - padding * 2.0
            - INVENTORY_TAB_HEIGHT
            - gap * 2.0
            - CRAFTING_FILTER_HEIGHT
            - CRAFTING_SCROLL_FOOTER_HEIGHT;
        let width = (self.panel.1[0] - padding * 2.0 - gap) / CRAFTING_COLUMNS as f32;
        let height = (available_height - gap * (CRAFTING_VISIBLE_ROWS - 1) as f32)
            / CRAFTING_VISIBLE_ROWS as f32;
        let column = index % CRAFTING_COLUMNS;
        let row = index / CRAFTING_COLUMNS;
        (
            [
                self.panel.0[0] - self.panel.1[0] * 0.5
                    + padding
                    + width * 0.5
                    + column as f32 * (width + gap),
                top + height * 0.5 + row as f32 * (height + gap),
            ],
            [width, height],
        )
    }

    fn craftable_filter_button(self) -> ([f32; 2], [f32; 2]) {
        let top = self.panel.0[1] - self.panel.1[1] * 0.5;
        (
            [
                self.panel.0[0],
                top + 8.0 + INVENTORY_TAB_HEIGHT + 4.0 + CRAFTING_FILTER_HEIGHT * 0.5,
            ],
            [self.panel.1[0] - 16.0, CRAFTING_FILTER_HEIGHT],
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum InventoryPage {
    #[default]
    Items,
    Crafting,
}

impl InventoryPage {
    const fn panel_height(self) -> f32 {
        match self {
            Self::Items => PERSONAL_ITEM_PANEL_HEIGHT,
            Self::Crafting => CRAFTING_PANEL_HEIGHT,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InventoryGui {
    open: bool,
    open_container: Option<ObjectId>,
    open_title: Option<&'static str>,
    controls: FurnitureControls,
    cursor_stack: Option<ItemStack>,
    page: InventoryPage,
    crafting_offset: usize,
    craftable_only: bool,
    advanced_targeting: bool,
    processor_slots: bool,
}

#[derive(Clone, Copy)]
pub struct FurnitureGuiState<'a> {
    container: Option<&'a ItemContainer>,
    active: bool,
    target_priority: Option<TargetPriority>,
    laser_aim: Option<LaserDrillAim>,
    battery_status: Option<BatteryStatus>,
    drill_depth_decimetres: Option<i32>,
    subsurface_survey: Option<SubsurfaceSurveyStatus>,
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
            laser_aim: None,
            battery_status: None,
            drill_depth_decimetres: None,
            subsurface_survey: None,
            turret_kill_count: None,
            lift_status: None,
            lift_station_configuration: None,
        }
    }

    pub const fn with_target_priority(mut self, value: Option<TargetPriority>) -> Self {
        self.target_priority = value;
        self
    }

    pub const fn with_laser_aim(mut self, value: Option<LaserDrillAim>) -> Self {
        self.laser_aim = value;
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

    pub const fn with_subsurface_survey(mut self, value: Option<SubsurfaceSurveyStatus>) -> Self {
        self.subsurface_survey = value;
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
pub struct SubsurfaceSurveyStatus {
    pub powered: bool,
    pub survey: Option<SubsurfaceSurvey>,
}

impl SubsurfaceSurveyStatus {
    pub const fn new(powered: bool, survey: Option<SubsurfaceSurvey>) -> Self {
        Self { powered, survey }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FurnitureControlAction {
    SetActive(ObjectId, bool),
    SetTargetPriority(ObjectId, TargetPriority),
    SetLaserDrillAim(ObjectId, LaserDrillAim),
    MoveCargoLift(ObjectId, CargoLiftDirection),
    SetLiftStationMode(ObjectId, LiftStationMode),
    SetLiftStationDeparture(ObjectId, CargoLiftDirection),
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

fn queue_inventory_tabs(
    renderer: &mut GuiRenderer,
    layout: CraftingLayout,
    page: InventoryPage,
    cursor: [f32; 2],
) {
    for (candidate, label, button) in [
        (InventoryPage::Items, "ITEMS", layout.tab_buttons()[0]),
        (InventoryPage::Crafting, "CRAFTING", layout.tab_buttons()[1]),
    ] {
        queue_control_button(renderer, button, label, candidate == page, cursor);
    }
}

fn queue_personal_item_panel(
    renderer: &mut GuiRenderer,
    registry: &ItemRegistry,
    selected: Option<ItemStack>,
    layout: CraftingLayout,
    cursor: [f32; 2],
) {
    renderer.queue_rect(layout.panel.0, layout.panel.1, [0.025, 0.055, 0.085, 0.96]);
    queue_inventory_tabs(renderer, layout, InventoryPage::Items, cursor);
    let content_height = layout.panel.1[1] - INVENTORY_TAB_HEIGHT - 20.0;
    let content = (
        [
            layout.panel.0[0],
            layout.panel.0[1] + (INVENTORY_TAB_HEIGHT + 4.0) * 0.5,
        ],
        [layout.panel.1[0] - 8.0, content_height],
    );
    queue_item_details(renderer, registry, selected, content);
}

fn queue_crafting_panel(
    renderer: &mut GuiRenderer,
    inventory: &Inventory,
    registry: &ItemRegistry,
    layout: CraftingLayout,
    offset: usize,
    craftable_only: bool,
    cursor: [f32; 2],
) {
    renderer.queue_rect(layout.panel.0, layout.panel.1, [0.025, 0.055, 0.085, 0.98]);
    queue_inventory_tabs(renderer, layout, InventoryPage::Crafting, cursor);
    queue_control_button(
        renderer,
        layout.craftable_filter_button(),
        if craftable_only {
            "CAN CRAFT: ON"
        } else {
            "CAN CRAFT: OFF"
        },
        craftable_only,
        cursor,
    );
    let recipes = filtered_crafting_recipes(inventory, registry, craftable_only);
    for (index, recipe) in recipes
        .iter()
        .copied()
        .skip(offset)
        .take(CRAFTING_VISIBLE_RECIPES)
        .enumerate()
    {
        let button = layout.recipe_button(index);
        let availability = recipe.can_craft(inventory, registry);
        let base: [f32; 4] = match availability {
            Ok(()) => [0.07, 0.30, 0.24, 1.0],
            Err(CraftingError::MissingIngredients) => [0.16, 0.12, 0.12, 1.0],
            Err(CraftingError::InventoryFull) => [0.30, 0.20, 0.06, 1.0],
            Err(CraftingError::UnknownItem(_)) => [0.22, 0.06, 0.12, 1.0],
        };
        let tint = if rect_contains(button.0, button.1, cursor) {
            [base[0] * 1.25, base[1] * 1.25, base[2] * 1.25, 1.0]
        } else {
            base
        };
        renderer.queue_rect(button.0, button.1, tint);
        let left = button.0[0] - button.1[0] * 0.5 + 6.0;
        let icon_size = (button.1[1] - 8.0).min(28.0);
        let icon_position = [left + icon_size * 0.5, button.0[1]];
        if let Some(definition) = registry.get(recipe.output.item) {
            renderer.queue_icon(
                definition.icon,
                icon_position,
                icon_size,
                if availability.is_ok() {
                    [1.0; 4]
                } else {
                    [0.65, 0.65, 0.65, 1.0]
                },
            );
            let text_left = left + icon_size + 6.0;
            let text_width = button.1[0] - icon_size - 18.0;
            let output = fit_text(
                &format!(
                    "{}X {}",
                    recipe.output.quantity,
                    definition.name.to_uppercase()
                ),
                1.0,
                text_width,
            );
            renderer.queue_text(
                &output,
                [text_left, button.0[1] - 11.0],
                1.0,
                [1.0, 0.82, 0.38, 1.0],
            );
            let ingredients = fit_text(
                &crafting_ingredients_text(recipe, registry),
                0.75,
                text_width,
            );
            renderer.queue_text(
                &ingredients,
                [text_left, button.0[1] + 4.0],
                0.75,
                if availability.is_ok() {
                    [0.68, 0.94, 0.78, 1.0]
                } else {
                    [0.92, 0.55, 0.48, 1.0]
                },
            );
        }
    }
    let first = (offset + 1).min(recipes.len());
    let last = (offset + CRAFTING_VISIBLE_RECIPES).min(recipes.len());
    let label = if recipes.is_empty() {
        "NO CRAFTABLE RECIPES".to_owned()
    } else {
        format!("RECIPES {first}-{last} / {}  •  SCROLL", recipes.len())
    };
    renderer.queue_text(
        &label,
        [
            layout.panel.0[0] - GuiRenderer::text_width(&label, 0.9) * 0.5,
            layout.panel.0[1] + layout.panel.1[1] * 0.5 - 14.0,
        ],
        0.9,
        [0.62, 0.72, 0.82, 1.0],
    );
}

fn filtered_crafting_recipes(
    inventory: &Inventory,
    registry: &ItemRegistry,
    craftable_only: bool,
) -> Vec<CraftingRecipe> {
    CRAFTING_RECIPES
        .iter()
        .copied()
        .filter(|recipe| !craftable_only || recipe.can_craft(inventory, registry).is_ok())
        .collect()
}

fn crafting_ingredients_text(recipe: CraftingRecipe, registry: &ItemRegistry) -> String {
    recipe
        .ingredients
        .iter()
        .filter_map(|ingredient| {
            registry.get(ingredient.item).map(|definition| {
                format!(
                    "{} {}",
                    ingredient.quantity,
                    definition
                        .name
                        .to_uppercase()
                        .replace(" BLOCK", "")
                        .replace("HARDENED COMPOSITE", "COMPOSITE")
                )
            })
        })
        .collect::<Vec<_>>()
        .join(" + ")
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
        ItemAction::PlaceCargoLift { .. } => Some("PLACE CARGO LIFT".to_owned()),
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
mod tests;
