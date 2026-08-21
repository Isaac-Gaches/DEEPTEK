use super::GuiRenderer;
use crate::{Contract, ContractCompany};

const MAX_VISIBLE_CONTRACTS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractsAction {
    Close,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ContractsGui;

impl ContractsGui {
    pub fn handle_click(
        self,
        cursor: [f32; 2],
        viewport: [f32; 2],
        contract_count: usize,
    ) -> Option<ContractsAction> {
        ContractLayout::new(viewport, contract_count)
            .close
            .contains(cursor)
            .then_some(ContractsAction::Close)
    }

    pub fn queue(
        self,
        renderer: &mut GuiRenderer,
        contracts: &[Contract],
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) {
        let visible_count = contracts.len().min(MAX_VISIBLE_CONTRACTS);
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
            "AVAILABLE CONTRACTS",
            layout.panel.centre[0],
            layout.panel.top() + 26.0,
            3.0,
            [0.82, 0.91, 1.0, 1.0],
        );
        queue_button(renderer, layout.close, cursor, "CLOSE");

        if contracts.is_empty() {
            queue_centred_text(
                renderer,
                "NO CONTRACTS AVAILABLE",
                layout.panel.centre[0],
                layout.panel.centre[1] - 8.0,
                2.0,
                [0.55, 0.63, 0.73, 1.0],
            );
            return;
        }

        for (index, contract) in contracts.iter().take(MAX_VISIBLE_CONTRACTS).enumerate() {
            let card = layout.card(index);
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
            } else {
                70.0
            };
            renderer.queue_text(
                &format!("REWARD {}", contract.reward),
                [card.left() + 16.0, card.top() + reward_top],
                1.5,
                [1.0, 0.78, 0.25, 1.0],
            );
        }

        if contracts.len() > MAX_VISIBLE_CONTRACTS {
            queue_centred_text(
                renderer,
                &format!("SHOWING {MAX_VISIBLE_CONTRACTS}/{}", contracts.len()),
                layout.panel.centre[0],
                layout.panel.bottom() - 27.0,
                1.3,
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
    rows_top: f32,
    row_height: f32,
}

impl ContractLayout {
    fn new(viewport: [f32; 2], contract_count: usize) -> Self {
        let visible_count = contract_count.clamp(1, MAX_VISIBLE_CONTRACTS);
        let width = (viewport[0] - 32.0).clamp(360.0, 760.0);
        let desired_height = 116.0 + visible_count as f32 * 104.0;
        let height = desired_height.min((viewport[1] - 32.0).max(300.0));
        let panel = Rect {
            centre: [viewport[0] * 0.5, viewport[1] * 0.5],
            size: [width, height],
        };
        let rows_top = panel.top() + 78.0;
        let available_rows_height = panel.bottom() - 24.0 - rows_top;
        let row_height = (available_rows_height / visible_count as f32 - 8.0).max(72.0);
        Self {
            panel,
            close: Rect {
                centre: [panel.right() - 48.0, panel.top() + 30.0],
                size: [72.0, 30.0],
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

    #[test]
    fn close_button_is_the_only_contract_action() {
        let viewport = [800.0, 600.0];
        let layout = ContractLayout::new(viewport, 3);
        let gui = ContractsGui;
        assert_eq!(
            gui.handle_click(layout.close.centre, viewport, 3),
            Some(ContractsAction::Close)
        );
        assert_eq!(gui.handle_click(layout.card(0).centre, viewport, 3), None);
    }

    #[test]
    fn long_requirements_are_truncated_cleanly() {
        assert_eq!(truncate("ABCDEFGHIJ", 8), "ABCDE...");
    }
}
