use super::GuiRenderer;
use crate::{SpecialistDefinition, SpecialistId, SpecialistRecord, specialist_definition};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialistAction {
    Close,
    OpenDetails,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SpecialistPage {
    #[default]
    Conversation,
    Details,
}

#[derive(Clone, Copy)]
pub struct SpecialistView<'a> {
    pub record: Option<&'a SpecialistRecord>,
}

#[derive(Debug, Default)]
pub struct SpecialistGui {
    specialist: Option<SpecialistId>,
    page: SpecialistPage,
}

impl SpecialistGui {
    pub const fn is_open(&self) -> bool {
        self.specialist.is_some()
    }

    pub const fn specialist(&self) -> Option<SpecialistId> {
        self.specialist
    }

    pub fn show(&mut self, specialist: SpecialistId) {
        self.specialist = Some(specialist);
        self.page = SpecialistPage::Conversation;
    }

    pub fn dismiss(&mut self) {
        self.specialist = None;
    }

    pub fn open_details(&mut self) {
        self.page = SpecialistPage::Details;
    }

    pub fn handle_click(
        &mut self,
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) -> Option<SpecialistAction> {
        self.definition()?;
        let layout = SpecialistLayout::new(viewport);
        if layout.close.contains(cursor) {
            return Some(SpecialistAction::Close);
        }
        match self.page {
            SpecialistPage::Conversation => layout
                .action
                .contains(cursor)
                .then_some(SpecialistAction::OpenDetails),
            SpecialistPage::Details => {
                if layout.back.contains(cursor) {
                    self.page = SpecialistPage::Conversation;
                }
                None
            }
        }
    }

    pub fn queue(
        &self,
        renderer: &mut GuiRenderer,
        view: SpecialistView<'_>,
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) {
        let Some(definition) = self.definition() else {
            return;
        };
        let layout = SpecialistLayout::new(viewport);
        renderer.queue_rect(
            [viewport[0] * 0.5, viewport[1] * 0.5],
            viewport,
            [0.0, 0.0, 0.0, 0.68],
        );
        renderer.queue_rect(
            layout.panel.centre,
            layout.panel.size,
            [0.025, 0.045, 0.065, 0.99],
        );
        renderer.queue_text(
            &definition.name.to_uppercase(),
            [layout.panel.left() + 22.0, layout.panel.top() + 19.0],
            2.35,
            definition.tint,
        );
        queue_button(renderer, layout.close, cursor, "CLOSE");
        match self.page {
            SpecialistPage::Conversation => {
                queue_conversation(renderer, view.record, definition, layout, cursor);
            }
            SpecialistPage::Details => {
                queue_details(renderer, view.record, definition, layout, cursor);
            }
        }
    }

    fn definition(&self) -> Option<&'static SpecialistDefinition> {
        specialist_definition(self.specialist?)
    }
}

fn queue_conversation(
    renderer: &mut GuiRenderer,
    record: Option<&SpecialistRecord>,
    definition: &SpecialistDefinition,
    layout: SpecialistLayout,
    cursor: [f32; 2],
) {
    renderer.queue_text(
        &format!(
            "{}  |  {}",
            definition.role.to_uppercase(),
            definition.company.short_name()
        ),
        [layout.panel.left() + 22.0, layout.panel.top() + 57.0],
        1.15,
        [0.58, 0.70, 0.80, 1.0],
    );
    renderer.queue_rect(
        layout.content.centre,
        layout.content.size,
        [0.045, 0.085, 0.115, 1.0],
    );
    renderer.queue_text(
        &format!("\"{}\"", wrap_text(definition.greeting, 56)),
        [layout.content.left() + 22.0, layout.content.top() + 28.0],
        1.55,
        [0.91, 0.96, 1.0, 1.0],
    );
    renderer.queue_text(
        &wrap_text(definition.description, 70),
        [layout.content.left() + 22.0, layout.content.bottom() - 70.0],
        1.05,
        [0.55, 0.66, 0.74, 1.0],
    );
    queue_happiness(renderer, record, layout);
    queue_button(renderer, layout.action, cursor, "BONUSES & DETAILS");
}

fn queue_details(
    renderer: &mut GuiRenderer,
    record: Option<&SpecialistRecord>,
    definition: &SpecialistDefinition,
    layout: SpecialistLayout,
    cursor: [f32; 2],
) {
    queue_button(renderer, layout.back, cursor, "BACK");
    renderer.queue_text(
        "SPECIALIST DETAILS",
        [layout.panel.left() + 112.0, layout.panel.top() + 68.0],
        1.35,
        definition.tint,
    );
    renderer.queue_rect(
        layout.content.centre,
        layout.content.size,
        [0.035, 0.07, 0.105, 1.0],
    );
    renderer.queue_text(
        &format!(
            "{}\n{}",
            definition.role.to_uppercase(),
            definition.company.display_name()
        ),
        [layout.content.left() + 22.0, layout.content.top() + 20.0],
        1.35,
        [0.90, 0.96, 1.0, 1.0],
    );
    renderer.queue_text(
        &wrap_text(definition.description, 72),
        [layout.content.left() + 22.0, layout.content.top() + 69.0],
        1.0,
        [0.58, 0.69, 0.78, 1.0],
    );
    let preferred = definition
        .preferred_biomes
        .iter()
        .map(|biome| biome.name())
        .collect::<Vec<_>>()
        .join(", ");
    renderer.queue_text(
        &format!("PREFERRED BIOME  {}", preferred.to_uppercase()),
        [layout.content.left() + 22.0, layout.content.top() + 111.0],
        0.95,
        [0.56, 0.82, 0.92, 1.0],
    );
    renderer.queue_text(
        "ACTIVE WORLD BONUSES",
        [layout.content.left() + 22.0, layout.content.top() + 137.0],
        1.25,
        definition.tint,
    );
    let bonus_top = layout.content.top() + 165.0;
    let gap = 8.0;
    let available_height = layout.content.bottom() - bonus_top - 18.0;
    let bonus_height = ((available_height - gap * 2.0) / 3.0).clamp(54.0, 72.0);
    for (index, bonus) in definition.bonuses.iter().copied().enumerate() {
        let card = Rect {
            centre: [
                layout.content.centre[0],
                bonus_top + bonus_height * 0.5 + index as f32 * (bonus_height + gap),
            ],
            size: [layout.content.size[0] - 44.0, bonus_height],
        };
        renderer.queue_rect(card.centre, card.size, [0.055, 0.115, 0.145, 1.0]);
        renderer.queue_text(
            bonus.label(),
            [card.left() + 14.0, card.top() + 10.0],
            1.1,
            [0.86, 0.94, 1.0, 1.0],
        );
        renderer.queue_text(
            &wrap_text(
                &bonus.description().to_uppercase(),
                (card.size[0] / 7.5).max(24.0) as usize,
            ),
            [card.left() + 14.0, card.top() + 33.0],
            0.9,
            [0.46, 0.88, 0.66, 1.0],
        );
    }
    queue_happiness(renderer, record, layout);
}

fn queue_happiness(
    renderer: &mut GuiRenderer,
    record: Option<&SpecialistRecord>,
    layout: SpecialistLayout,
) {
    let happiness = record.map_or("HAPPINESS --".to_owned(), |record| {
        format!("HAPPINESS {}%", record.happiness())
    });
    renderer.queue_text(
        &happiness,
        [layout.panel.left() + 22.0, layout.panel.bottom() - 31.0],
        1.2,
        [0.48, 0.90, 0.64, 1.0],
    );
}

fn queue_button(renderer: &mut GuiRenderer, rect: Rect, cursor: [f32; 2], label: &str) {
    let colour = if rect.contains(cursor) {
        [0.20, 0.43, 0.50, 1.0]
    } else {
        [0.10, 0.27, 0.34, 1.0]
    };
    renderer.queue_rect(rect.centre, rect.size, colour);
    renderer.queue_text(
        label,
        [
            rect.centre[0] - GuiRenderer::text_width(label, 1.25) * 0.5,
            rect.centre[1] - 5.5,
        ],
        1.25,
        [1.0; 4],
    );
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

#[derive(Clone, Copy)]
struct SpecialistLayout {
    panel: Rect,
    content: Rect,
    close: Rect,
    back: Rect,
    action: Rect,
}

impl SpecialistLayout {
    fn new(viewport: [f32; 2]) -> Self {
        let panel = Rect {
            centre: [viewport[0] * 0.5, viewport[1] * 0.5],
            size: [
                (viewport[0] - 24.0).clamp(420.0, 780.0),
                (viewport[1] - 24.0).clamp(400.0, 590.0),
            ],
        };
        let content = Rect {
            centre: [panel.centre[0], panel.centre[1] - 8.0],
            size: [panel.size[0] - 44.0, panel.size[1] - 166.0],
        };
        Self {
            panel,
            content,
            close: Rect {
                centre: [panel.right() - 48.0, panel.top() + 30.0],
                size: [72.0, 30.0],
            },
            back: Rect {
                centre: [panel.left() + 48.0, panel.top() + 76.0],
                size: [72.0, 30.0],
            },
            action: Rect {
                centre: [panel.right() - 130.0, panel.bottom() - 25.0],
                size: [210.0, 38.0],
            },
        }
    }
}

#[derive(Clone, Copy)]
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
    fn details_button_opens_the_bonus_page() {
        let viewport = [900.0, 650.0];
        let layout = SpecialistLayout::new(viewport);
        let mut gui = SpecialistGui::default();
        gui.show(SpecialistId::ENGINEER);

        assert_eq!(
            gui.handle_click(layout.action.centre, viewport),
            Some(SpecialistAction::OpenDetails)
        );
        gui.open_details();
        assert_eq!(gui.page, SpecialistPage::Details);
    }

    #[test]
    fn every_specialist_exposes_bonuses() {
        for definition in crate::BUILT_IN_SPECIALISTS {
            assert!(!definition.bonuses.is_empty());
        }
    }

    #[test]
    fn back_returns_to_conversation_without_closing() {
        let viewport = [900.0, 650.0];
        let layout = SpecialistLayout::new(viewport);
        let mut gui = SpecialistGui::default();
        gui.show(SpecialistId::QUARTERMASTER);
        gui.open_details();

        assert_eq!(gui.handle_click(layout.back.centre, viewport), None);
        assert_eq!(gui.page, SpecialistPage::Conversation);
        assert!(gui.is_open());
    }
}
