mod contracts_tab;
mod corporations_tab;
mod specialists_tab;

use super::GuiRenderer;
use crate::{
    AcceptContractError, ContractBoard, ContractCompany, CorporationProgress, DeliverySystem,
    ItemId, ItemRegistry, MACHINE_OFFERS, ObjectId, PurchaseError, RecruitSpecialistError,
    SpecialistId, SpecialistTerminalView,
};
use contracts_tab::queue_contracts_tab;
use corporations_tab::queue_corporations_tab;
use specialists_tab::queue_specialists_tab;

const MAX_VISIBLE_OFFERS: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcurementAction {
    Close,
    Buy(ItemId),
    AcceptContract(usize),
    RecruitSpecialist(SpecialistId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalFeedback {
    Ordered,
    InsufficientFunds,
    CorporationLevelRequired,
    QueueFull,
    Unavailable,
    ContractAccepted,
    ContractLimitReached,
    ContractUnavailable,
    SpecialistRecruited,
    SpecialistAlreadyRecruited,
    HouseUnsuitable,
    SpecialistUnavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProcurementTab {
    #[default]
    Catalogue,
    Contracts,
    Corporations,
    Specialists,
}

#[derive(Clone, Copy)]
pub struct ProcurementView<'a> {
    registry: &'a ItemRegistry,
    deliveries: &'a DeliverySystem,
    contracts: &'a ContractBoard,
    corporation_progress: &'a CorporationProgress,
    money: u64,
    specialists: &'a SpecialistTerminalView,
}

impl<'a> ProcurementView<'a> {
    pub const fn new(
        registry: &'a ItemRegistry,
        deliveries: &'a DeliverySystem,
        contracts: &'a ContractBoard,
        corporation_progress: &'a CorporationProgress,
        money: u64,
        specialists: &'a SpecialistTerminalView,
    ) -> Self {
        Self {
            registry,
            deliveries,
            contracts,
            corporation_progress,
            money,
            specialists,
        }
    }
}

#[derive(Debug, Default)]
pub struct ProcurementGui {
    open: bool,
    selected: usize,
    scroll_offset: usize,
    sort_by_company: bool,
    purchasable_only: bool,
    contract_selected: usize,
    contract_scroll_offset: usize,
    specialist_selected: usize,
    specialist_scroll_offset: usize,
    tab: ProcurementTab,
    feedback: Option<TerminalFeedback>,
    terminal: Option<ObjectId>,
}

impl ProcurementGui {
    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn show(&mut self) {
        self.open = true;
        self.feedback = None;
        self.ensure_selection_visible();
    }

    pub fn show_for(&mut self, terminal: ObjectId) {
        self.terminal = Some(terminal);
        self.show();
    }

    pub const fn terminal(&self) -> Option<ObjectId> {
        self.terminal
    }

    pub fn dismiss(&mut self) {
        self.open = false;
        self.terminal = None;
    }

    pub fn set_purchase_result(&mut self, result: Result<(), PurchaseError>) {
        self.feedback = Some(match result {
            Ok(()) => TerminalFeedback::Ordered,
            Err(PurchaseError::InsufficientFunds) => TerminalFeedback::InsufficientFunds,
            Err(PurchaseError::CorporationLevelRequired) => {
                TerminalFeedback::CorporationLevelRequired
            }
            Err(PurchaseError::QueueFull) => TerminalFeedback::QueueFull,
            Err(PurchaseError::ItemNotOffered) => TerminalFeedback::Unavailable,
        });
    }

    pub fn set_contract_result(
        &mut self,
        result: Result<(), AcceptContractError>,
        available_count: usize,
    ) {
        self.feedback = Some(match result {
            Ok(()) => TerminalFeedback::ContractAccepted,
            Err(AcceptContractError::ActiveLimitReached) => TerminalFeedback::ContractLimitReached,
            Err(AcceptContractError::InvalidContract) => TerminalFeedback::ContractUnavailable,
        });
        self.contract_selected = self
            .contract_selected
            .min(available_count.saturating_sub(1));
        self.contract_scroll_offset = self
            .contract_scroll_offset
            .min(available_count.saturating_sub(1));
    }

    pub fn set_specialist_result(&mut self, result: Result<(), RecruitSpecialistError>) {
        self.feedback = Some(match result {
            Ok(()) => TerminalFeedback::SpecialistRecruited,
            Err(RecruitSpecialistError::AlreadyRecruited) => {
                TerminalFeedback::SpecialistAlreadyRecruited
            }
            Err(RecruitSpecialistError::UnsuitableHouse) => TerminalFeedback::HouseUnsuitable,
            Err(RecruitSpecialistError::UnknownSpecialist)
            | Err(RecruitSpecialistError::InvalidTerminal) => {
                TerminalFeedback::SpecialistUnavailable
            }
        });
    }

    pub fn handle_click(
        &mut self,
        cursor: [f32; 2],
        viewport: [f32; 2],
        contracts: &ContractBoard,
    ) -> Option<ProcurementAction> {
        self.handle_click_with_catalogue(
            cursor,
            viewport,
            contracts,
            CorporationProgress::from_experience([u32::MAX; ContractCompany::ALL.len()]),
            u64::MAX,
        )
    }

    pub fn handle_click_with_catalogue(
        &mut self,
        cursor: [f32; 2],
        viewport: [f32; 2],
        contracts: &ContractBoard,
        corporation_progress: CorporationProgress,
        money: u64,
    ) -> Option<ProcurementAction> {
        if !self.open {
            return None;
        }
        let layout = ProcurementLayout::new(viewport);
        if layout.close.contains(cursor) {
            return Some(ProcurementAction::Close);
        }
        if layout.catalogue_tab.contains(cursor) {
            self.tab = ProcurementTab::Catalogue;
            self.feedback = None;
            return None;
        }
        if layout.contracts_tab.contains(cursor) {
            self.tab = ProcurementTab::Contracts;
            self.feedback = None;
            return None;
        }
        if layout.corporations_tab.contains(cursor) {
            self.tab = ProcurementTab::Corporations;
            self.feedback = None;
            return None;
        }
        if layout.specialists_tab.contains(cursor) {
            self.tab = ProcurementTab::Specialists;
            self.feedback = None;
            return None;
        }
        match self.tab {
            ProcurementTab::Catalogue => {
                if layout.corporation_sort.contains(cursor) {
                    self.sort_by_company = !self.sort_by_company;
                    self.reset_catalogue_view(corporation_progress, money);
                    return None;
                }
                if layout.purchasable_filter.contains(cursor) {
                    self.purchasable_only = !self.purchasable_only;
                    self.reset_catalogue_view(corporation_progress, money);
                    return None;
                }
                let offer_indices = self.catalogue_offer_indices(corporation_progress, money);
                for visible_index in 0..layout.visible_rows {
                    let Some(&offer_index) = offer_indices.get(self.scroll_offset + visible_index)
                    else {
                        break;
                    };
                    if layout.offer_row(visible_index).contains(cursor) {
                        self.selected = offer_index;
                        self.feedback = None;
                        return None;
                    }
                }
                if layout.action.contains(cursor)
                    && offer_indices.contains(&self.selected)
                    && let Some(offer) = MACHINE_OFFERS
                        .get(self.selected)
                        .filter(|offer| offer.can_purchase(corporation_progress, money))
                {
                    return Some(ProcurementAction::Buy(offer.item));
                }
            }
            ProcurementTab::Contracts => {
                for visible_index in 0..layout.visible_rows {
                    let contract_index = self.contract_scroll_offset + visible_index;
                    if contract_index >= contracts.available().len() {
                        break;
                    }
                    if layout.offer_row(visible_index).contains(cursor) {
                        self.contract_selected = contract_index;
                        self.feedback = None;
                        return None;
                    }
                }
                if layout.action.contains(cursor)
                    && contracts.available().get(self.contract_selected).is_some()
                {
                    return Some(ProcurementAction::AcceptContract(self.contract_selected));
                }
            }
            ProcurementTab::Corporations => {}
            ProcurementTab::Specialists => {
                for visible_index in 0..layout.visible_rows {
                    let index = self.specialist_scroll_offset + visible_index;
                    if index >= crate::BUILT_IN_SPECIALISTS.len() {
                        break;
                    }
                    if layout.offer_row(visible_index).contains(cursor) {
                        self.specialist_selected = index;
                        self.feedback = None;
                        return None;
                    }
                }
                if layout.action.contains(cursor)
                    && let Some(definition) =
                        crate::BUILT_IN_SPECIALISTS.get(self.specialist_selected)
                {
                    return Some(ProcurementAction::RecruitSpecialist(definition.id));
                }
            }
        }
        None
    }

    pub fn scroll(&mut self, direction: f32, viewport: [f32; 2], contracts: &ContractBoard) {
        self.scroll_with_catalogue(
            direction,
            viewport,
            contracts,
            CorporationProgress::default(),
            u64::MAX,
        );
    }

    pub fn scroll_with_catalogue(
        &mut self,
        direction: f32,
        viewport: [f32; 2],
        contracts: &ContractBoard,
        corporation_progress: CorporationProgress,
        money: u64,
    ) {
        if !self.open || direction == 0.0 {
            return;
        }
        let visible_rows = ProcurementLayout::new(viewport).visible_rows;
        let catalogue_count = self
            .catalogue_offer_indices(corporation_progress, money)
            .len();
        let (offset, item_count) = match self.tab {
            ProcurementTab::Catalogue => (&mut self.scroll_offset, catalogue_count),
            ProcurementTab::Contracts => (
                &mut self.contract_scroll_offset,
                contracts.available().len(),
            ),
            ProcurementTab::Corporations => return,
            ProcurementTab::Specialists => (
                &mut self.specialist_scroll_offset,
                crate::BUILT_IN_SPECIALISTS.len(),
            ),
        };
        let maximum = item_count.saturating_sub(visible_rows);
        if direction < 0.0 {
            *offset = offset.saturating_add(1).min(maximum);
        } else {
            *offset = offset.saturating_sub(1);
        }
    }

    pub fn queue(
        &self,
        renderer: &mut GuiRenderer,
        view: ProcurementView<'_>,
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) {
        if !self.open {
            return;
        }
        let layout = ProcurementLayout::new(viewport);
        renderer.queue_rect(
            [viewport[0] * 0.5, viewport[1] * 0.5],
            viewport,
            [0.0, 0.0, 0.0, 0.68],
        );
        renderer.queue_rect(
            layout.panel.centre,
            layout.panel.size,
            [0.02, 0.04, 0.065, 0.99],
        );
        renderer.queue_text(
            "PROCUREMENT TERMINAL",
            [layout.panel.left() + 22.0, layout.panel.top() + 19.0],
            2.5,
            [0.42, 0.94, 1.0, 1.0],
        );
        queue_button(renderer, layout.close, cursor, "CLOSE", true);

        queue_tab(
            renderer,
            layout.catalogue_tab,
            cursor,
            "MACHINES",
            self.tab == ProcurementTab::Catalogue,
        );
        queue_tab(
            renderer,
            layout.contracts_tab,
            cursor,
            "CONTRACTS",
            self.tab == ProcurementTab::Contracts,
        );
        queue_tab(
            renderer,
            layout.corporations_tab,
            cursor,
            "CORPS",
            self.tab == ProcurementTab::Corporations,
        );
        queue_tab(
            renderer,
            layout.specialists_tab,
            cursor,
            "SPECIALISTS",
            self.tab == ProcurementTab::Specialists,
        );

        if self.tab == ProcurementTab::Contracts {
            queue_contracts_tab(
                renderer,
                view.contracts,
                self.contract_selected,
                self.contract_scroll_offset,
                self.feedback,
                layout,
                cursor,
            );
            return;
        }
        if self.tab == ProcurementTab::Corporations {
            queue_corporations_tab(renderer, view.corporation_progress, layout);
            return;
        }
        if self.tab == ProcurementTab::Specialists {
            queue_specialists_tab(
                renderer,
                view.specialists,
                self.specialist_selected,
                self.specialist_scroll_offset,
                self.feedback,
                layout,
                cursor,
            );
            return;
        }

        let offer_indices = self.catalogue_offer_indices(*view.corporation_progress, view.money);
        queue_filter_button(
            renderer,
            layout.corporation_sort,
            cursor,
            if self.sort_by_company {
                "SORT: CORPORATION"
            } else {
                "SORT: DEFAULT"
            },
            self.sort_by_company,
        );
        queue_filter_button(
            renderer,
            layout.purchasable_filter,
            cursor,
            if self.purchasable_only {
                "CAN BUY: ON"
            } else {
                "CAN BUY: OFF"
            },
            self.purchasable_only,
        );
        renderer.queue_text(
            "MACHINE CATALOGUE",
            [layout.list.left(), layout.list.top() - 25.0],
            1.45,
            [0.62, 0.72, 0.82, 1.0],
        );
        for visible_index in 0..layout.visible_rows {
            let Some(&offer_index) = offer_indices.get(self.scroll_offset + visible_index) else {
                break;
            };
            let Some(offer) = MACHINE_OFFERS.get(offer_index) else {
                break;
            };
            let unlocked = offer.is_unlocked(*view.corporation_progress);
            let row = layout.offer_row(visible_index);
            let selected = offer_index == self.selected;
            let hovered = row.contains(cursor);
            let colour = if selected {
                [0.08, 0.26, 0.32, 1.0]
            } else if hovered {
                [0.09, 0.15, 0.21, 1.0]
            } else {
                [0.045, 0.085, 0.12, 1.0]
            };
            renderer.queue_rect(row.centre, row.size, colour);
            if let Some(definition) = view.registry.get(offer.item) {
                renderer.queue_icon(
                    definition.icon,
                    [row.left() + 29.0, row.centre[1]],
                    44.0,
                    if unlocked {
                        [1.0; 4]
                    } else {
                        [0.5, 0.5, 0.5, 1.0]
                    },
                );
                renderer.queue_text(
                    &truncate(&definition.name.to_uppercase(), 23),
                    [row.left() + 58.0, row.top() + 11.0],
                    1.25,
                    if unlocked {
                        [0.92, 0.96, 1.0, 1.0]
                    } else {
                        [0.55, 0.58, 0.62, 1.0]
                    },
                );
            }
            renderer.queue_text(
                &format!("${}", offer.price),
                [row.left() + 58.0, row.top() + 35.0],
                1.25,
                [1.0, 0.76, 0.25, 1.0],
            );
            let company = format!(
                "{} L{}",
                offer.company.short_name(),
                offer.minimum_company_level
            );
            let company_scale = 0.78;
            renderer.queue_text(
                &company,
                [
                    row.right() - 10.0 - GuiRenderer::text_width(&company, company_scale),
                    row.top() + 36.0,
                ],
                company_scale,
                company_colour(offer.company),
            );
        }
        if offer_indices.len() > layout.visible_rows {
            renderer.queue_text(
                &format!(
                    "{}-{} / {}   SCROLL",
                    self.scroll_offset + 1,
                    (self.scroll_offset + layout.visible_rows).min(offer_indices.len()),
                    offer_indices.len()
                ),
                [layout.list.left(), layout.list.bottom() + 45.0],
                1.15,
                [0.50, 0.62, 0.72, 1.0],
            );
        }

        let selected = offer_indices
            .contains(&self.selected)
            .then_some(self.selected)
            .or_else(|| offer_indices.first().copied());
        if let Some(offer) = selected.and_then(|selected| MACHINE_OFFERS.get(selected)) {
            renderer.queue_rect(
                layout.details.centre,
                layout.details.size,
                [0.035, 0.07, 0.105, 1.0],
            );
            if let Some(definition) = view.registry.get(offer.item) {
                renderer.queue_slot(
                    [layout.details.centre[0], layout.details.top() + 87.0],
                    126.0,
                    [0.55, 0.82, 0.90, 1.0],
                );
                renderer.queue_icon(
                    definition.icon,
                    [layout.details.centre[0], layout.details.top() + 87.0],
                    108.0,
                    [1.0; 4],
                );
                queue_centred_text(
                    renderer,
                    &definition.name.to_uppercase(),
                    layout.details.centre[0],
                    layout.details.top() + 164.0,
                    1.7,
                    [0.92, 0.97, 1.0, 1.0],
                );
            }
            queue_centred_text(
                renderer,
                offer.company.display_name(),
                layout.details.centre[0],
                layout.details.top() + 186.0,
                0.95,
                company_colour(offer.company),
            );
            let current_level = view.corporation_progress.level(offer.company);
            queue_centred_text(
                renderer,
                &format!(
                    "REQUIRES LEVEL {}   CURRENT {}",
                    offer.minimum_company_level, current_level
                ),
                layout.details.centre[0],
                layout.details.top() + 201.0,
                0.85,
                if current_level >= offer.minimum_company_level {
                    [0.48, 0.88, 0.65, 1.0]
                } else {
                    [1.0, 0.42, 0.30, 1.0]
                },
            );
            let description_width = (layout.details.size[0] / 9.0).max(16.0) as usize;
            renderer.queue_text(
                &wrap_text(offer.description, description_width),
                [layout.details.left() + 18.0, layout.details.top() + 224.0],
                1.25,
                [0.68, 0.77, 0.84, 1.0],
            );
            queue_centred_text(
                renderer,
                &format!("PRICE ${}", offer.price),
                layout.details.centre[0],
                layout.action.top() - 34.0,
                1.5,
                [1.0, 0.78, 0.28, 1.0],
            );
            queue_button(
                renderer,
                layout.action,
                cursor,
                if !offer.is_unlocked(*view.corporation_progress) {
                    "CORPORATION LEVEL REQUIRED"
                } else if view.money < offer.price {
                    "INSUFFICIENT FUNDS"
                } else {
                    "ADD TO DELIVERY QUEUE"
                },
                offer.can_purchase(*view.corporation_progress, view.money),
            );
        }

        let queue_text = match view.deliveries.seconds_until_next() {
            Some(seconds) => format!(
                "QUEUE {}   NEXT DROP {}S",
                view.deliveries.pending_count(),
                seconds.ceil() as u32
            ),
            None => "QUEUE EMPTY".to_owned(),
        };
        renderer.queue_text(
            &queue_text,
            [layout.panel.left() + 22.0, layout.panel.bottom() - 27.0],
            1.3,
            [0.48, 0.83, 0.91, 1.0],
        );
        if let Some(feedback) = self.feedback {
            let (text, colour) = match feedback {
                TerminalFeedback::Ordered => (
                    "ORDER ACCEPTED - DROP IN 15 SECONDS",
                    [0.40, 1.0, 0.58, 1.0],
                ),
                TerminalFeedback::InsufficientFunds => {
                    ("INSUFFICIENT FUNDS", [1.0, 0.32, 0.26, 1.0])
                }
                TerminalFeedback::CorporationLevelRequired => {
                    ("CORPORATION LEVEL TOO LOW", [1.0, 0.32, 0.26, 1.0])
                }
                TerminalFeedback::QueueFull => ("DELIVERY QUEUE FULL", [1.0, 0.32, 0.26, 1.0]),
                TerminalFeedback::Unavailable => ("MACHINE UNAVAILABLE", [1.0, 0.32, 0.26, 1.0]),
                TerminalFeedback::ContractAccepted
                | TerminalFeedback::ContractLimitReached
                | TerminalFeedback::ContractUnavailable => return,
                TerminalFeedback::SpecialistRecruited
                | TerminalFeedback::SpecialistAlreadyRecruited
                | TerminalFeedback::HouseUnsuitable
                | TerminalFeedback::SpecialistUnavailable => return,
            };
            queue_centred_text(
                renderer,
                text,
                layout.details.centre[0],
                layout.action.bottom() + 12.0,
                1.1,
                colour,
            );
        }
    }

    fn ensure_selection_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + MAX_VISIBLE_OFFERS {
            self.scroll_offset = self.selected + 1 - MAX_VISIBLE_OFFERS;
        }
    }

    fn catalogue_offer_indices(&self, progress: CorporationProgress, money: u64) -> Vec<usize> {
        let mut offers: Vec<_> = MACHINE_OFFERS
            .iter()
            .enumerate()
            .filter(|(_, offer)| !self.purchasable_only || offer.can_purchase(progress, money))
            .map(|(index, _)| index)
            .collect();
        if self.sort_by_company {
            offers.sort_by_key(|&index| (MACHINE_OFFERS[index].company.index(), index));
        }
        offers
    }

    fn reset_catalogue_view(&mut self, progress: CorporationProgress, money: u64) {
        self.scroll_offset = 0;
        if let Some(first) = self.catalogue_offer_indices(progress, money).first() {
            self.selected = *first;
        }
        self.feedback = None;
    }
}

fn queue_filter_button(
    renderer: &mut GuiRenderer,
    button: Rect,
    cursor: [f32; 2],
    label: &str,
    active: bool,
) {
    let colour = if active {
        [0.08, 0.34, 0.32, 1.0]
    } else if button.contains(cursor) {
        [0.10, 0.22, 0.29, 1.0]
    } else {
        [0.055, 0.12, 0.17, 1.0]
    };
    renderer.queue_rect(button.centre, button.size, colour);
    queue_centred_text(
        renderer,
        label,
        button.centre[0],
        button.centre[1] - 4.0,
        0.85,
        [0.88, 0.96, 1.0, 1.0],
    );
}

fn queue_button(
    renderer: &mut GuiRenderer,
    button: Rect,
    cursor: [f32; 2],
    label: &str,
    enabled: bool,
) {
    let colour = if !enabled {
        [0.10, 0.12, 0.14, 1.0]
    } else if button.contains(cursor) {
        [0.20, 0.43, 0.50, 1.0]
    } else {
        [0.10, 0.27, 0.34, 1.0]
    };
    renderer.queue_rect(button.centre, button.size, colour);
    queue_centred_text(
        renderer,
        label,
        button.centre[0],
        button.centre[1] - 5.5,
        1.35,
        if enabled {
            [1.0; 4]
        } else {
            [0.45, 0.48, 0.50, 1.0]
        },
    );
}

fn queue_tab(renderer: &mut GuiRenderer, tab: Rect, cursor: [f32; 2], label: &str, selected: bool) {
    let colour = if selected {
        [0.08, 0.34, 0.42, 1.0]
    } else if tab.contains(cursor) {
        [0.10, 0.20, 0.27, 1.0]
    } else {
        [0.055, 0.11, 0.16, 1.0]
    };
    renderer.queue_rect(tab.centre, tab.size, colour);
    queue_centred_text(
        renderer,
        label,
        tab.centre[0],
        tab.centre[1] - 5.0,
        1.3,
        [0.88, 0.96, 1.0, 1.0],
    );
}

fn company_colour(company: ContractCompany) -> [f32; 4] {
    match company {
        ContractCompany::DeepTekIndustries => [0.15, 0.82, 0.92, 1.0],
        ContractCompany::VanguardDefence => [0.95, 0.34, 0.25, 1.0],
        ContractCompany::AstraSurveyCorp => [0.68, 0.52, 1.0, 1.0],
    }
}

fn queue_centred_text(
    renderer: &mut GuiRenderer,
    text: &str,
    centre_x: f32,
    top: f32,
    scale: f32,
    tint: [f32; 4],
) {
    renderer.queue_text(
        text,
        [centre_x - GuiRenderer::text_width(text, scale) * 0.5, top],
        scale,
        tint,
    );
}

fn truncate(text: &str, maximum: usize) -> String {
    if text.chars().count() <= maximum {
        return text.to_owned();
    }
    let mut output: String = text.chars().take(maximum.saturating_sub(3)).collect();
    output.push_str("...");
    output
}

fn wrap_text(text: &str, maximum: usize) -> String {
    let mut output = String::new();
    let mut line_length = 0;
    for word in text.split_whitespace() {
        let separator = usize::from(line_length > 0);
        if line_length + separator + word.len() > maximum && line_length > 0 {
            output.push('\n');
            line_length = 0;
        } else if line_length > 0 {
            output.push(' ');
            line_length += 1;
        }
        output.push_str(word);
        line_length += word.len();
    }
    output
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ProcurementLayout {
    panel: Rect,
    list: Rect,
    details: Rect,
    close: Rect,
    catalogue_tab: Rect,
    contracts_tab: Rect,
    corporations_tab: Rect,
    specialists_tab: Rect,
    action: Rect,
    corporation_sort: Rect,
    purchasable_filter: Rect,
    visible_rows: usize,
    row_height: f32,
}

impl ProcurementLayout {
    fn new(viewport: [f32; 2]) -> Self {
        let panel = Rect {
            centre: [viewport[0] * 0.5, viewport[1] * 0.5],
            size: [
                (viewport[0] - 24.0).clamp(360.0, 960.0),
                (viewport[1] - 24.0).clamp(360.0, 620.0),
            ],
        };
        let content_top = panel.top() + 112.0;
        let content_bottom = panel.bottom() - 55.0;
        let content_height = (content_bottom - content_top).max(220.0);
        let list_width = (panel.size[0] * 0.43).clamp(150.0, 380.0);
        let gap = 16.0;
        let details_width = panel.size[0] - 44.0 - list_width - gap;
        let row_height = 64.0;
        let visible_rows = ((content_height + 7.0) / (row_height + 7.0))
            .floor()
            .clamp(1.0, MAX_VISIBLE_OFFERS as f32) as usize;
        let list_height =
            visible_rows as f32 * row_height + visible_rows.saturating_sub(1) as f32 * 7.0;
        let list_left = panel.left() + 22.0;
        let details_left = list_left + list_width + gap;
        let details = Rect {
            centre: [
                details_left + details_width * 0.5,
                content_top + content_height * 0.5,
            ],
            size: [details_width, content_height],
        };
        let tab_gap = 8.0;
        let tab_width = ((panel.size[0] - 44.0 - tab_gap * 3.0) / 4.0).max(68.0);
        let first_tab_x = panel.left() + 22.0 + tab_width * 0.5;
        let filter_width = (list_width - 7.0) * 0.5;
        Self {
            panel,
            list: Rect {
                centre: [
                    list_left + list_width * 0.5,
                    content_top + list_height * 0.5,
                ],
                size: [list_width, list_height],
            },
            details,
            close: Rect {
                centre: [panel.right() - 48.0, panel.top() + 30.0],
                size: [72.0, 30.0],
            },
            catalogue_tab: Rect {
                centre: [first_tab_x, panel.top() + 76.0],
                size: [tab_width, 32.0],
            },
            contracts_tab: Rect {
                centre: [first_tab_x + tab_width + tab_gap, panel.top() + 76.0],
                size: [tab_width, 32.0],
            },
            corporations_tab: Rect {
                centre: [
                    first_tab_x + (tab_width + tab_gap) * 2.0,
                    panel.top() + 76.0,
                ],
                size: [tab_width, 32.0],
            },
            specialists_tab: Rect {
                centre: [
                    first_tab_x + (tab_width + tab_gap) * 3.0,
                    panel.top() + 76.0,
                ],
                size: [tab_width, 32.0],
            },
            action: Rect {
                centre: [details.centre[0], details.bottom() - 57.0],
                size: [(details.size[0] - 36.0).max(120.0), 38.0],
            },
            corporation_sort: Rect {
                centre: [
                    list_left + filter_width * 0.5,
                    content_top + list_height + 22.0,
                ],
                size: [filter_width, 24.0],
            },
            purchasable_filter: Rect {
                centre: [
                    list_left + filter_width * 1.5 + 7.0,
                    content_top + list_height + 22.0,
                ],
                size: [filter_width, 24.0],
            },
            visible_rows,
            row_height,
        }
    }

    fn offer_row(self, visible_index: usize) -> Rect {
        Rect {
            centre: [
                self.list.centre[0],
                self.list.top()
                    + self.row_height * 0.5
                    + visible_index as f32 * (self.row_height + 7.0),
            ],
            size: [self.list.size[0], self.row_height],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    centre: [f32; 2],
    size: [f32; 2],
}

impl Rect {
    fn left(self) -> f32 {
        self.centre[0] - self.size[0] * 0.5
    }

    fn right(self) -> f32 {
        self.centre[0] + self.size[0] * 0.5
    }

    fn top(self) -> f32 {
        self.centre[1] - self.size[1] * 0.5
    }

    fn bottom(self) -> f32 {
        self.centre[1] + self.size[1] * 0.5
    }

    fn contains(self, point: [f32; 2]) -> bool {
        (self.left()..=self.right()).contains(&point[0])
            && (self.top()..=self.bottom()).contains(&point[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicking_an_offer_selects_it_and_buy_uses_the_selection() {
        let viewport = [900.0, 650.0];
        let layout = ProcurementLayout::new(viewport);
        let mut gui = ProcurementGui::default();
        let contracts = ContractBoard::with_built_ins();
        gui.show();

        assert_eq!(
            gui.handle_click(layout.offer_row(2).centre, viewport, &contracts),
            None
        );
        assert_eq!(
            gui.handle_click(layout.action.centre, viewport, &contracts),
            Some(ProcurementAction::Buy(MACHINE_OFFERS[2].item))
        );
    }

    #[test]
    fn catalogue_scroll_is_bounded() {
        let viewport = [900.0, 650.0];
        let mut gui = ProcurementGui::default();
        let contracts = ContractBoard::with_built_ins();
        gui.show();
        for _ in 0..100 {
            gui.scroll(-1.0, viewport, &contracts);
        }
        let maximum = MACHINE_OFFERS
            .len()
            .saturating_sub(ProcurementLayout::new(viewport).visible_rows);
        assert_eq!(gui.scroll_offset, maximum);
        for _ in 0..100 {
            gui.scroll(1.0, viewport, &contracts);
        }
        assert_eq!(gui.scroll_offset, 0);
        let layout = ProcurementLayout::new(viewport);
        assert!(layout.corporation_sort.top() > layout.list.bottom());
        assert!(layout.purchasable_filter.top() > layout.list.bottom());
        assert!(layout.corporation_sort.bottom() < layout.panel.bottom());
    }

    #[test]
    fn catalogue_can_sort_by_corporation_and_filter_to_currently_purchasable_offers() {
        let viewport = [900.0, 650.0];
        let layout = ProcurementLayout::new(viewport);
        let contracts = ContractBoard::with_built_ins();
        let mut gui = ProcurementGui::default();
        gui.show();
        let progress = CorporationProgress::default();

        assert!(
            gui.catalogue_offer_indices(progress, 10_000).contains(&1),
            "the locked red bore remains visible without the filter"
        );
        assert_eq!(
            gui.handle_click_with_catalogue(
                layout.corporation_sort.centre,
                viewport,
                &contracts,
                progress,
                10_000,
            ),
            None
        );
        let sorted = gui.catalogue_offer_indices(progress, 10_000);
        assert!(sorted.windows(2).all(|pair| {
            MACHINE_OFFERS[pair[0]].company.index() <= MACHINE_OFFERS[pair[1]].company.index()
        }));

        gui.handle_click_with_catalogue(
            layout.purchasable_filter.centre,
            viewport,
            &contracts,
            progress,
            2_000,
        );
        let purchasable = gui.catalogue_offer_indices(progress, 2_000);
        assert!(!purchasable.is_empty());
        assert!(
            purchasable
                .iter()
                .all(|&index| MACHINE_OFFERS[index].can_purchase(progress, 2_000))
        );
        assert!(!purchasable.contains(&1));
    }

    #[test]
    fn locked_catalogue_offer_is_visible_but_has_no_purchase_action() {
        let viewport = [900.0, 650.0];
        let layout = ProcurementLayout::new(viewport);
        let contracts = ContractBoard::with_built_ins();
        let mut gui = ProcurementGui::default();
        gui.show();
        gui.selected = 1;

        assert_eq!(
            MACHINE_OFFERS[1].item,
            ItemId::RED_SHAFT_BORE,
            "test assumes the red bore is the advanced Astra offer"
        );
        assert_eq!(
            gui.handle_click_with_catalogue(
                layout.action.centre,
                viewport,
                &contracts,
                CorporationProgress::default(),
                10_000,
            ),
            None
        );
        let unlocked = CorporationProgress::from_experience([0, 0, 500]);
        assert_eq!(
            gui.handle_click_with_catalogue(
                layout.action.centre,
                viewport,
                &contracts,
                unlocked,
                10_000,
            ),
            Some(ProcurementAction::Buy(ItemId::RED_SHAFT_BORE))
        );
    }

    #[test]
    fn contracts_tab_selects_and_accepts_available_contracts() {
        let viewport = [900.0, 650.0];
        let layout = ProcurementLayout::new(viewport);
        let contracts = ContractBoard::with_built_ins();
        let mut gui = ProcurementGui::default();
        gui.show();

        assert_eq!(
            gui.handle_click(layout.contracts_tab.centre, viewport, &contracts),
            None
        );
        assert_eq!(
            gui.handle_click(layout.offer_row(1).centre, viewport, &contracts),
            None
        );
        assert_eq!(
            gui.handle_click(layout.action.centre, viewport, &contracts),
            Some(ProcurementAction::AcceptContract(1))
        );
    }

    #[test]
    fn corporations_tab_is_read_only_and_does_not_scroll_catalogue() {
        let viewport = [900.0, 650.0];
        let layout = ProcurementLayout::new(viewport);
        let contracts = ContractBoard::with_built_ins();
        let mut gui = ProcurementGui::default();
        gui.show();
        gui.scroll(-1.0, viewport, &contracts);
        let catalogue_offset = gui.scroll_offset;

        assert_eq!(
            gui.handle_click(layout.corporations_tab.centre, viewport, &contracts),
            None
        );
        assert_eq!(gui.tab, ProcurementTab::Corporations);
        assert_eq!(
            gui.handle_click(layout.action.centre, viewport, &contracts),
            None
        );
        gui.scroll(-1.0, viewport, &contracts);
        assert_eq!(gui.scroll_offset, catalogue_offset);
    }

    #[test]
    fn specialists_tab_selects_a_unique_specialist() {
        let viewport = [900.0, 650.0];
        let layout = ProcurementLayout::new(viewport);
        let contracts = ContractBoard::with_built_ins();
        let mut gui = ProcurementGui::default();
        gui.show();

        assert_eq!(
            gui.handle_click(layout.specialists_tab.centre, viewport, &contracts),
            None
        );
        assert_eq!(
            gui.handle_click(layout.offer_row(1).centre, viewport, &contracts),
            None
        );
        assert_eq!(
            gui.handle_click(layout.action.centre, viewport, &contracts),
            Some(ProcurementAction::RecruitSpecialist(
                SpecialistId::GEOLOGIST
            ))
        );
    }

    #[test]
    fn descriptions_wrap_without_losing_words() {
        assert_eq!(wrap_text("ONE TWO THREE FOUR", 9), "ONE TWO\nTHREE\nFOUR");
    }
}
