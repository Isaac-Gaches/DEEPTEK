use super::{
    GuiRenderer, ProcurementLayout, TerminalFeedback, company_colour, queue_button,
    queue_centred_text, truncate, wrap_text,
};
use crate::{ContractBoard, MAX_ACTIVE_CONTRACTS};

pub(super) fn queue_contracts_tab(
    renderer: &mut GuiRenderer,
    board: &ContractBoard,
    selected: usize,
    scroll_offset: usize,
    feedback: Option<TerminalFeedback>,
    layout: ProcurementLayout,
    cursor: [f32; 2],
) {
    renderer.queue_text(
        "AVAILABLE CONTRACTS",
        [layout.list.left(), layout.list.top() - 25.0],
        1.45,
        [0.62, 0.72, 0.82, 1.0],
    );
    if board.available().is_empty() {
        queue_centred_text(
            renderer,
            "NO CONTRACTS AVAILABLE",
            layout.list.centre[0],
            layout.list.centre[1] - 7.0,
            1.25,
            [0.50, 0.62, 0.72, 1.0],
        );
    }
    for visible_index in 0..layout.visible_rows {
        let contract_index = scroll_offset + visible_index;
        let Some(contract) = board.available().get(contract_index) else {
            break;
        };
        let row = layout.offer_row(visible_index);
        let row_colour = if contract_index == selected {
            [0.08, 0.26, 0.32, 1.0]
        } else if row.contains(cursor) {
            [0.09, 0.15, 0.21, 1.0]
        } else {
            [0.045, 0.085, 0.12, 1.0]
        };
        renderer.queue_rect(row.centre, row.size, row_colour);
        renderer.queue_rect(
            [row.left() + 2.0, row.centre[1]],
            [4.0, row.size[1]],
            company_colour(contract.company),
        );
        renderer.queue_text(
            contract.company.display_name(),
            [row.left() + 14.0, row.top() + 10.0],
            1.0,
            company_colour(contract.company),
        );
        renderer.queue_text(
            &truncate(&contract.requirement, 31),
            [row.left() + 14.0, row.top() + 30.0],
            1.15,
            [0.91, 0.96, 1.0, 1.0],
        );
        renderer.queue_text(
            &format!(
                "REWARD ${}  +{} XP",
                contract.reward, contract.experience_reward
            ),
            [row.left() + 14.0, row.top() + 48.0],
            0.95,
            [1.0, 0.78, 0.25, 1.0],
        );
    }
    if board.available().len() > layout.visible_rows {
        renderer.queue_text(
            &format!(
                "{}-{} / {}   SCROLL",
                scroll_offset + 1,
                (scroll_offset + layout.visible_rows).min(board.available().len()),
                board.available().len()
            ),
            [layout.list.left(), layout.list.bottom() + 9.0],
            1.15,
            [0.50, 0.62, 0.72, 1.0],
        );
    }

    renderer.queue_rect(
        layout.details.centre,
        layout.details.size,
        [0.035, 0.07, 0.105, 1.0],
    );
    if let Some(contract) = board.available().get(selected) {
        queue_centred_text(
            renderer,
            contract.company.display_name(),
            layout.details.centre[0],
            layout.details.top() + 38.0,
            1.6,
            company_colour(contract.company),
        );
        renderer.queue_text(
            &wrap_text(
                &contract.requirement,
                (layout.details.size[0] / 9.0).max(16.0) as usize,
            ),
            [layout.details.left() + 20.0, layout.details.top() + 92.0],
            1.45,
            [0.90, 0.96, 1.0, 1.0],
        );
        queue_centred_text(
            renderer,
            &format!(
                "REWARD ${}  +{} XP",
                contract.reward, contract.experience_reward
            ),
            layout.details.centre[0],
            layout.action.top() - 38.0,
            1.55,
            [1.0, 0.78, 0.25, 1.0],
        );
        let has_room = board.active().len() < MAX_ACTIVE_CONTRACTS;
        queue_button(
            renderer,
            layout.action,
            cursor,
            if has_room {
                "ACCEPT CONTRACT"
            } else {
                "ACTIVE LIMIT REACHED"
            },
            has_room,
        );
    }

    renderer.queue_text(
        &format!(
            "ACTIVE {}/{}   AVAILABLE {}",
            board.active().len(),
            MAX_ACTIVE_CONTRACTS,
            board.available().len()
        ),
        [layout.panel.left() + 22.0, layout.panel.bottom() - 27.0],
        1.3,
        [0.48, 0.83, 0.91, 1.0],
    );
    if let Some(feedback) = feedback {
        let (text, colour) = match feedback {
            TerminalFeedback::ContractAccepted => ("CONTRACT ACCEPTED", [0.40, 1.0, 0.58, 1.0]),
            TerminalFeedback::ContractLimitReached => {
                ("FOUR ACTIVE CONTRACT LIMIT", [1.0, 0.32, 0.26, 1.0])
            }
            TerminalFeedback::ContractUnavailable => {
                ("CONTRACT UNAVAILABLE", [1.0, 0.32, 0.26, 1.0])
            }
            TerminalFeedback::Ordered
            | TerminalFeedback::InsufficientFunds
            | TerminalFeedback::CorporationLevelRequired
            | TerminalFeedback::QueueFull
            | TerminalFeedback::Unavailable
            | TerminalFeedback::SpecialistRecruited
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
