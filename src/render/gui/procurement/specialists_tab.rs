use super::{
    GuiRenderer, ProcurementLayout, TerminalFeedback, company_colour, queue_button,
    queue_centred_text, truncate, wrap_text,
};
use crate::SpecialistTerminalView;

pub(super) fn queue_specialists_tab(
    renderer: &mut GuiRenderer,
    view: &SpecialistTerminalView,
    selected: usize,
    scroll_offset: usize,
    feedback: Option<TerminalFeedback>,
    layout: ProcurementLayout,
    cursor: [f32; 2],
) {
    renderer.queue_text(
        "AVAILABLE SPECIALISTS",
        [layout.list.left(), layout.list.top() - 25.0],
        1.45,
        [0.62, 0.72, 0.82, 1.0],
    );
    for visible_index in 0..layout.visible_rows {
        let index = scroll_offset + visible_index;
        let Some(specialist) = view.specialists.get(index) else {
            break;
        };
        let row = layout.offer_row(visible_index);
        let colour = if index == selected {
            [0.08, 0.26, 0.32, 1.0]
        } else if row.contains(cursor) {
            [0.09, 0.15, 0.21, 1.0]
        } else {
            [0.045, 0.085, 0.12, 1.0]
        };
        renderer.queue_rect(row.centre, row.size, colour);
        renderer.queue_rect(
            [row.left() + 2.0, row.centre[1]],
            [4.0, row.size[1]],
            company_colour(specialist.definition.company),
        );
        renderer.queue_text(
            &truncate(&specialist.definition.name.to_uppercase(), 25),
            [row.left() + 14.0, row.top() + 9.0],
            1.15,
            [0.92, 0.96, 1.0, 1.0],
        );
        renderer.queue_text(
            &specialist.definition.role.to_uppercase(),
            [row.left() + 14.0, row.top() + 29.0],
            1.0,
            company_colour(specialist.definition.company),
        );
        let status = if let Some(happiness) = specialist.happiness {
            format!("RESIDENT  HAPPINESS {happiness}%")
        } else {
            specialist.definition.company.short_name().to_owned()
        };
        renderer.queue_text(
            &status,
            [row.left() + 14.0, row.top() + 47.0],
            0.88,
            [0.55, 0.72, 0.80, 1.0],
        );
    }

    renderer.queue_rect(
        layout.details.centre,
        layout.details.size,
        [0.035, 0.07, 0.105, 1.0],
    );
    if let Some(specialist) = view.specialists.get(selected) {
        queue_centred_text(
            renderer,
            &specialist.definition.name.to_uppercase(),
            layout.details.centre[0],
            layout.details.top() + 34.0,
            1.65,
            [0.92, 0.97, 1.0, 1.0],
        );
        queue_centred_text(
            renderer,
            &format!(
                "{} - {}",
                specialist.definition.role.to_uppercase(),
                specialist.definition.company.short_name()
            ),
            layout.details.centre[0],
            layout.details.top() + 61.0,
            1.0,
            company_colour(specialist.definition.company),
        );
        renderer.queue_text(
            &wrap_text(
                specialist.definition.description,
                (layout.details.size[0] / 9.0).max(16.0) as usize,
            ),
            [layout.details.left() + 20.0, layout.details.top() + 96.0],
            1.25,
            [0.70, 0.80, 0.87, 1.0],
        );

        let house_ready = view.requirements.is_valid();
        let house_text = if house_ready {
            format!("HOUSE READY - {} TILES", view.interior_cells)
        } else {
            format!(
                "HOUSE NEEDS: {}",
                view.requirements
                    .missing_labels()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        renderer.queue_text(
            &wrap_text(
                &house_text,
                (layout.details.size[0] / 8.0).max(18.0) as usize,
            ),
            [layout.details.left() + 20.0, layout.action.top() - 62.0],
            1.05,
            if house_ready {
                [0.40, 1.0, 0.58, 1.0]
            } else {
                [1.0, 0.48, 0.30, 1.0]
            },
        );
        let enabled = house_ready && !specialist.recruited;
        queue_button(
            renderer,
            layout.action,
            cursor,
            if specialist.recruited {
                "ALREADY RECRUITED"
            } else if house_ready {
                "INVITE SPECIALIST"
            } else {
                "HOUSE NOT SUITABLE"
            },
            enabled,
        );
    }

    if let Some(feedback) = feedback {
        let (text, colour) = match feedback {
            TerminalFeedback::SpecialistRecruited => {
                ("SPECIALIST MOVING IN", [0.40, 1.0, 0.58, 1.0])
            }
            TerminalFeedback::SpecialistAlreadyRecruited => {
                ("SPECIALIST ALREADY RECRUITED", [1.0, 0.48, 0.30, 1.0])
            }
            TerminalFeedback::HouseUnsuitable => {
                ("HOUSE REQUIREMENTS NOT MET", [1.0, 0.32, 0.26, 1.0])
            }
            TerminalFeedback::SpecialistUnavailable => {
                ("SPECIALIST UNAVAILABLE", [1.0, 0.32, 0.26, 1.0])
            }
            _ => return,
        };
        queue_centred_text(
            renderer,
            text,
            layout.details.centre[0],
            layout.action.bottom() + 12.0,
            1.05,
            colour,
        );
    }
}
