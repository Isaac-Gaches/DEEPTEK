use super::{GuiRenderer, ProcurementLayout, company_colour, queue_centred_text};
use crate::{
    CORPORATION_LEVEL_THRESHOLDS, ContractCompany, CorporationProgress, MAX_CORPORATION_LEVEL,
};

pub(super) fn queue_corporations_tab(
    renderer: &mut GuiRenderer,
    progress: &CorporationProgress,
    layout: ProcurementLayout,
) {
    renderer.queue_text(
        "CORPORATION STANDING",
        [layout.list.left(), layout.list.top() - 25.0],
        1.45,
        [0.62, 0.72, 0.82, 1.0],
    );
    let left = layout.list.left();
    let right = layout.details.right();
    let width = right - left;
    let gap = 14.0;
    let available_height = layout.details.size[1];
    let card_height = ((available_height - gap * 2.0) / 3.0).min(126.0);
    for (index, company) in ContractCompany::ALL.into_iter().enumerate() {
        let top = layout.details.top() + index as f32 * (card_height + gap);
        let centre = [left + width * 0.5, top + card_height * 0.5];
        let colour = company_colour(company);
        renderer.queue_rect(centre, [width, card_height], [0.035, 0.07, 0.105, 1.0]);
        renderer.queue_rect([left + 3.0, centre[1]], [6.0, card_height], colour);
        renderer.queue_text(
            company.display_name(),
            [left + 22.0, top + 16.0],
            1.45,
            colour,
        );
        let level = progress.level(company);
        renderer.queue_text(
            &format!("LEVEL {level} / {MAX_CORPORATION_LEVEL}"),
            [right - 150.0, top + 16.0],
            1.2,
            [0.90, 0.96, 1.0, 1.0],
        );
        let experience = progress.experience(company);
        let current_threshold = CORPORATION_LEVEL_THRESHOLDS[usize::from(level)];
        let (label, fraction) = if let Some(next) = progress.next_level_experience(company) {
            let level_experience = experience.saturating_sub(current_threshold);
            let required = next - current_threshold;
            (
                format!("{experience} XP   {level_experience} / {required} TO NEXT LEVEL"),
                level_experience as f32 / required as f32,
            )
        } else {
            (format!("{experience} XP   MAXIMUM STANDING"), 1.0)
        };
        let bar_left = left + 22.0;
        let bar_width = width - 44.0;
        let bar_y = top + card_height - 16.0;
        renderer.queue_rect(
            [bar_left + bar_width * 0.5, bar_y],
            [bar_width, 14.0],
            [0.08, 0.11, 0.14, 1.0],
        );
        let fill_width = bar_width * fraction.clamp(0.0, 1.0);
        if fill_width > 0.0 {
            renderer.queue_rect(
                [bar_left + fill_width * 0.5, bar_y],
                [fill_width, 14.0],
                colour,
            );
        }
        queue_centred_text(
            renderer,
            &label,
            centre[0],
            bar_y - 24.0,
            0.95,
            [0.65, 0.76, 0.84, 1.0],
        );
    }
}
