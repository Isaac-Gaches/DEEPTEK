use super::{GuiRenderer, transmissions::wrap_text};
use crate::{Contract, ContractCompany, Transmission};

const MAX_VISIBLE_CONTRACTS: usize = 4;
const MAX_VISIBLE_TRANSMISSIONS: usize = 3;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContractsTab {
    #[default]
    Contracts,
    Transmissions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractsAction {
    Close,
    CollectReward(usize),
}

#[derive(Clone, Debug, Default)]
pub struct ContractsGui {
    tab: ContractsTab,
    contract_offset: usize,
    transmission_offset: usize,
}

impl ContractsGui {
    pub const fn tab(&self) -> ContractsTab {
        self.tab
    }

    pub fn show_contracts(&mut self) {
        self.tab = ContractsTab::Contracts;
    }

    pub fn handle_click(
        &mut self,
        cursor: [f32; 2],
        viewport: [f32; 2],
        contracts: &[Contract],
        transmissions: &[Transmission],
    ) -> Option<ContractsAction> {
        let row_count = match self.tab {
            ContractsTab::Contracts => contracts.len().min(MAX_VISIBLE_CONTRACTS),
            ContractsTab::Transmissions => transmissions.len().min(MAX_VISIBLE_TRANSMISSIONS),
        };
        let layout = ContractLayout::new(viewport, row_count);
        if layout.close.contains(cursor) {
            return Some(ContractsAction::Close);
        }
        if layout.contracts_tab.contains(cursor) {
            self.tab = ContractsTab::Contracts;
            return None;
        }
        if layout.transmissions_tab.contains(cursor) {
            self.tab = ContractsTab::Transmissions;
            return None;
        }
        if self.tab != ContractsTab::Contracts {
            return None;
        }
        let offset = self
            .contract_offset
            .min(contracts.len().saturating_sub(MAX_VISIBLE_CONTRACTS));
        contracts
            .iter()
            .enumerate()
            .skip(offset)
            .take(MAX_VISIBLE_CONTRACTS)
            .find_map(|(index, contract)| {
                let row = index - offset;
                (contract.is_complete() && layout.claim_button(row).contains(cursor))
                    .then_some(ContractsAction::CollectReward(index))
            })
    }

    pub fn queue(
        &self,
        renderer: &mut GuiRenderer,
        contracts: &[Contract],
        transmissions: &[Transmission],
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) {
        let visible_count = match self.tab {
            ContractsTab::Contracts => contracts.len().min(MAX_VISIBLE_CONTRACTS),
            ContractsTab::Transmissions => transmissions.len().min(MAX_VISIBLE_TRANSMISSIONS),
        };
        let layout = ContractLayout::new(viewport, visible_count);
        renderer.queue_rect(
            [viewport[0] * 0.5, viewport[1] * 0.5],
            viewport,
            [0.0, 0.0, 0.0, 0.58],
        );
        renderer.queue_rect(
            layout.panel.centre,
            layout.panel.size,
            [0.025, 0.04, 0.07, 0.98],
        );
        queue_centred_text(
            renderer,
            "DEEPTEK PROSPECTOR PROGRAM",
            layout.panel.centre[0],
            layout.panel.top() + 26.0,
            3.0,
            [0.82, 0.91, 1.0, 1.0],
        );
        queue_button(renderer, layout.close, cursor, "CLOSE");
        queue_tab(
            renderer,
            layout.contracts_tab,
            cursor,
            "CONTRACTS",
            self.tab == ContractsTab::Contracts,
        );
        queue_tab(
            renderer,
            layout.transmissions_tab,
            cursor,
            &format!("TRANSMISSIONS ({})", transmissions.len()),
            self.tab == ContractsTab::Transmissions,
        );

        if self.tab == ContractsTab::Transmissions {
            self.queue_transmissions(renderer, transmissions, &layout);
            return;
        }

        if contracts.is_empty() {
            queue_centred_text(
                renderer,
                "NO ACTIVE CONTRACTS",
                layout.panel.centre[0],
                layout.panel.centre[1] + 18.0,
                2.0,
                [0.55, 0.63, 0.73, 1.0],
            );
            return;
        }

        let offset = self
            .contract_offset
            .min(contracts.len().saturating_sub(MAX_VISIBLE_CONTRACTS));
        for (row, contract) in contracts
            .iter()
            .skip(offset)
            .take(MAX_VISIBLE_CONTRACTS)
            .enumerate()
        {
            let card = layout.card(row);
            renderer.queue_rect(card.centre, card.size, [0.055, 0.08, 0.125, 1.0]);
            renderer.queue_rect(
                [card.left() + 2.0, card.centre[1]],
                [4.0, card.size[1]],
                company_colour(contract.company),
            );
            renderer.queue_text(
                contract.company.display_name(),
                [card.left() + 16.0, card.top() + 12.0],
                1.5,
                company_colour(contract.company),
            );
            let max_requirement_characters = ((card.size[0] - 32.0) / 10.8).max(1.0) as usize;
            renderer.queue_text(
                &truncate(&contract.requirement, max_requirement_characters),
                [card.left() + 16.0, card.top() + 40.0],
                1.8,
                [0.94, 0.97, 1.0, 1.0],
            );
            let reward_top = if let Some(progress) = contract.export_progress() {
                let colour = if progress.is_complete() {
                    [0.35, 0.95, 0.55, 1.0]
                } else {
                    [0.50, 0.72, 0.88, 1.0]
                };
                renderer.queue_text(
                    &format!("EXPORTED {}/{}", progress.exported, progress.required),
                    [card.left() + 16.0, card.top() + 66.0],
                    1.35,
                    colour,
                );
                84.0
            } else if let Some(progress) = contract.mine_progress() {
                let colour = if progress.is_complete() {
                    [0.35, 0.95, 0.55, 1.0]
                } else {
                    [0.50, 0.72, 0.88, 1.0]
                };
                renderer.queue_text(
                    &format!("MINED {}/{}", progress.mined, progress.required),
                    [card.left() + 16.0, card.top() + 66.0],
                    1.35,
                    colour,
                );
                84.0
            } else if let Some(progress) = contract.build_and_export_progress() {
                let colour = if progress.is_complete() {
                    [0.35, 0.95, 0.55, 1.0]
                } else {
                    [0.50, 0.72, 0.88, 1.0]
                };
                renderer.queue_text(
                    &format!(
                        "PLACED {}/{}  EXPORTED {}/{}",
                        progress.placed,
                        progress.required_placements,
                        progress.exported,
                        progress.required_exports
                    ),
                    [card.left() + 16.0, card.top() + 66.0],
                    1.25,
                    colour,
                );
                84.0
            } else if let Some(progress) = contract.program_progress() {
                let colour = if progress.is_complete() {
                    [0.35, 0.95, 0.55, 1.0]
                } else {
                    [0.50, 0.72, 0.88, 1.0]
                };
                renderer.queue_text(
                    &format!("OBJECTIVES {}/{}", progress.completed, progress.required),
                    [card.left() + 16.0, card.top() + 66.0],
                    1.35,
                    colour,
                );
                84.0
            } else {
                70.0
            };
            renderer.queue_text(
                &format!(
                    "REWARD ${}  +{} XP",
                    contract.reward, contract.experience_reward
                ),
                [card.left() + 16.0, card.top() + reward_top],
                1.5,
                [1.0, 0.78, 0.25, 1.0],
            );
            if contract.is_complete() {
                queue_claim_button(renderer, layout.claim_button(row), cursor, contract.reward);
            }
        }

        if contracts.len() > MAX_VISIBLE_CONTRACTS {
            queue_centred_text(
                renderer,
                &format!(
                    "SHOWING {}-{} / {}  (SCROLL)",
                    offset + 1,
                    (offset + MAX_VISIBLE_CONTRACTS).min(contracts.len()),
                    contracts.len()
                ),
                layout.panel.centre[0],
                layout.panel.bottom() - 27.0,
                1.3,
                [0.55, 0.63, 0.73, 1.0],
            );
        }
    }

    pub fn scroll(&mut self, direction: f32, contract_count: usize, transmission_count: usize) {
        if direction == 0.0 {
            return;
        }
        let (offset, maximum) = match self.tab {
            ContractsTab::Contracts => (
                &mut self.contract_offset,
                contract_count.saturating_sub(MAX_VISIBLE_CONTRACTS),
            ),
            ContractsTab::Transmissions => (
                &mut self.transmission_offset,
                transmission_count.saturating_sub(MAX_VISIBLE_TRANSMISSIONS),
            ),
        };
        if direction > 0.0 {
            *offset = offset.saturating_sub(1);
        } else {
            *offset = (*offset + 1).min(maximum);
        }
    }

    fn queue_transmissions(
        &self,
        renderer: &mut GuiRenderer,
        transmissions: &[Transmission],
        layout: &ContractLayout,
    ) {
        if transmissions.is_empty() {
            queue_centred_text(
                renderer,
                "NO TRANSMISSIONS RECEIVED",
                layout.panel.centre[0],
                layout.panel.centre[1] + 18.0,
                2.0,
                [0.55, 0.63, 0.73, 1.0],
            );
            return;
        }
        let offset = self
            .transmission_offset
            .min(transmissions.len().saturating_sub(1));
        for (row, transmission) in transmissions
            .iter()
            .rev()
            .skip(offset)
            .take(MAX_VISIBLE_TRANSMISSIONS)
            .enumerate()
        {
            let card = layout.card(row);
            renderer.queue_rect(card.centre, card.size, [0.045, 0.075, 0.115, 1.0]);
            renderer.queue_rect(
                [card.left() + 2.0, card.centre[1]],
                [4.0, card.size[1]],
                [0.15, 0.82, 0.92, 1.0],
            );
            renderer.queue_text(
                &format!(
                    "#{:03} // {}",
                    transmission.sequence(),
                    transmission.sender()
                ),
                [card.left() + 16.0, card.top() + 10.0],
                1.25,
                [0.45, 0.90, 1.0, 1.0],
            );
            renderer.queue_text(
                transmission.subject(),
                [card.left() + 16.0, card.top() + 31.0],
                1.55,
                [0.94, 0.97, 1.0, 1.0],
            );
            let maximum_width = card.size[0] - 32.0;
            let maximum_lines = ((card.size[1] - 61.0) / 18.0).max(1.0) as usize;
            for (line, text) in wrap_text(transmission.body(), 1.25, maximum_width)
                .into_iter()
                .take(maximum_lines)
                .enumerate()
            {
                renderer.queue_text(
                    &text,
                    [card.left() + 16.0, card.top() + 56.0 + line as f32 * 18.0],
                    1.25,
                    [0.72, 0.80, 0.88, 1.0],
                );
            }
        }
        if transmissions.len() > MAX_VISIBLE_TRANSMISSIONS {
            queue_centred_text(
                renderer,
                "MOUSE WHEEL TO BROWSE TRANSMISSION LOG",
                layout.panel.centre[0],
                layout.panel.bottom() - 22.0,
                1.1,
                [0.55, 0.63, 0.73, 1.0],
            );
        }
    }
}

fn company_colour(company: ContractCompany) -> [f32; 4] {
    match company {
        ContractCompany::DeepTekIndustries => [0.15, 0.82, 0.92, 1.0],
        ContractCompany::VanguardDefence => [0.95, 0.34, 0.25, 1.0],
        ContractCompany::AstraSurveyCorp => [0.68, 0.52, 1.0, 1.0],
    }
}

fn truncate(text: &str, maximum_characters: usize) -> String {
    if text.chars().count() <= maximum_characters {
        return text.to_owned();
    }
    if maximum_characters <= 3 {
        return text.chars().take(maximum_characters).collect();
    }
    let mut output: String = text.chars().take(maximum_characters - 3).collect();
    output.push_str("...");
    output
}

fn queue_button(renderer: &mut GuiRenderer, button: Rect, cursor: [f32; 2], label: &str) {
    let colour = if button.contains(cursor) {
        [0.26, 0.36, 0.50, 1.0]
    } else {
        [0.15, 0.22, 0.32, 1.0]
    };
    renderer.queue_rect(button.centre, button.size, colour);
    renderer.queue_text(
        label,
        [
            button.centre[0] - GuiRenderer::text_width(label, 1.5) * 0.5,
            button.centre[1] - 5.25,
        ],
        1.5,
        [1.0; 4],
    );
}

fn queue_tab(
    renderer: &mut GuiRenderer,
    button: Rect,
    cursor: [f32; 2],
    label: &str,
    selected: bool,
) {
    let colour = if selected {
        [0.10, 0.42, 0.56, 1.0]
    } else if button.contains(cursor) {
        [0.16, 0.30, 0.42, 1.0]
    } else {
        [0.09, 0.16, 0.24, 1.0]
    };
    renderer.queue_rect(button.centre, button.size, colour);
    renderer.queue_text(
        label,
        [
            button.centre[0] - GuiRenderer::text_width(label, 1.25) * 0.5,
            button.centre[1] - 4.5,
        ],
        1.25,
        [0.92, 0.97, 1.0, 1.0],
    );
}

fn queue_claim_button(renderer: &mut GuiRenderer, button: Rect, cursor: [f32; 2], reward: u64) {
    let colour = if button.contains(cursor) {
        [0.22, 0.58, 0.36, 1.0]
    } else {
        [0.12, 0.40, 0.25, 1.0]
    };
    renderer.queue_rect(button.centre, button.size, colour);
    let label = format!("COLLECT ${reward}");
    renderer.queue_text(
        &label,
        [
            button.centre[0] - GuiRenderer::text_width(&label, 1.2) * 0.5,
            button.centre[1] - 4.25,
        ],
        1.2,
        [0.90, 1.0, 0.92, 1.0],
    );
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct ContractLayout {
    panel: Rect,
    close: Rect,
    contracts_tab: Rect,
    transmissions_tab: Rect,
    rows_top: f32,
    row_height: f32,
}

impl ContractLayout {
    fn new(viewport: [f32; 2], contract_count: usize) -> Self {
        let visible_count = contract_count.clamp(1, MAX_VISIBLE_CONTRACTS);
        let width = (viewport[0] - 32.0).clamp(360.0, 760.0);
        let desired_height = 148.0 + visible_count as f32 * 112.0;
        let height = desired_height.min((viewport[1] - 32.0).max(300.0));
        let panel = Rect {
            centre: [viewport[0] * 0.5, viewport[1] * 0.5],
            size: [width, height],
        };
        let rows_top = panel.top() + 108.0;
        let available_rows_height = panel.bottom() - 24.0 - rows_top;
        let row_height = (available_rows_height / visible_count as f32 - 8.0).max(72.0);
        let tab_width = ((panel.size[0] - 52.0) * 0.5).max(120.0);
        Self {
            panel,
            close: Rect {
                centre: [panel.right() - 48.0, panel.top() + 30.0],
                size: [72.0, 30.0],
            },
            contracts_tab: Rect {
                centre: [panel.left() + 20.0 + tab_width * 0.5, panel.top() + 76.0],
                size: [tab_width, 32.0],
            },
            transmissions_tab: Rect {
                centre: [panel.right() - 20.0 - tab_width * 0.5, panel.top() + 76.0],
                size: [tab_width, 32.0],
            },
            rows_top,
            row_height,
        }
    }

    fn card(self, index: usize) -> Rect {
        Rect {
            centre: [
                self.panel.centre[0],
                self.rows_top + self.row_height * 0.5 + index as f32 * (self.row_height + 8.0),
            ],
            size: [self.panel.size[0] - 40.0, self.row_height],
        }
    }

    fn claim_button(self, index: usize) -> Rect {
        let card = self.card(index);
        Rect {
            centre: [card.right() - 94.0, card.centre[1]],
            size: [164.0, 34.0],
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
        let half = [self.size[0] * 0.5, self.size[1] * 0.5];
        (self.centre[0] - half[0]..=self.centre[0] + half[0]).contains(&point[0])
            && (self.centre[1] - half[1]..=self.centre[1] + half[1]).contains(&point[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_export_to_contracts, built_in_contracts};

    #[test]
    fn incomplete_contract_only_exposes_close() {
        let viewport = [800.0, 600.0];
        let mut gui = ContractsGui::default();
        let contracts = built_in_contracts();
        let transmissions = [];
        let layout = ContractLayout::new(viewport, contracts.len());
        assert_eq!(
            gui.handle_click(layout.close.centre, viewport, &contracts, &transmissions),
            Some(ContractsAction::Close)
        );
        assert_eq!(
            gui.handle_click(
                layout.claim_button(0).centre,
                viewport,
                &contracts,
                &transmissions,
            ),
            None
        );
    }

    #[test]
    fn completed_contract_exposes_reward_collection() {
        let viewport = [800.0, 600.0];
        let layout = ContractLayout::new(viewport, 1);
        let mut gui = ContractsGui::default();
        let mut contracts = vec![Contract::export_items(
            "ONE STONE",
            50,
            ContractCompany::DeepTekIndustries,
            crate::ItemId::STONE_BLOCK,
            1,
        )];
        apply_export_to_contracts(&mut contracts, crate::ItemId::STONE_BLOCK, 1);
        assert_eq!(
            gui.handle_click(layout.claim_button(0).centre, viewport, &contracts, &[]),
            Some(ContractsAction::CollectReward(0))
        );
    }

    #[test]
    fn long_requirements_are_truncated_cleanly() {
        assert_eq!(truncate("ABCDEFGHIJ", 8), "ABCDE...");
    }

    #[test]
    fn transmission_tab_is_selectable() {
        let viewport = [800.0, 600.0];
        let layout = ContractLayout::new(viewport, 1);
        let mut gui = ContractsGui::default();
        gui.handle_click(layout.transmissions_tab.centre, viewport, &[], &[]);
        assert_eq!(gui.tab(), ContractsTab::Transmissions);
    }
}
