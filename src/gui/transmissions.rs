use super::GuiRenderer;
use crate::Transmission;

const BANNER_MAX_WIDTH: f32 = 920.0;
const BANNER_MARGIN: f32 = 14.0;
const BODY_SCALE: f32 = 1.35;
const DISMISS_BUTTON_SIZE: [f32; 2] = [92.0, 28.0];

pub fn queue_incoming_transmission(
    renderer: &mut GuiRenderer,
    transmission: Option<&Transmission>,
    cursor: [f32; 2],
    viewport: [f32; 2],
) {
    let Some(transmission) = transmission else {
        return;
    };
    let layout = IncomingLayout::new(transmission, viewport);
    let lines = wrap_text(transmission.body(), BODY_SCALE, layout.width - 36.0);

    renderer.queue_rect(
        layout.centre,
        [layout.width + 4.0, layout.height + 4.0],
        [0.0, 0.78, 0.92, 0.72],
    );
    renderer.queue_rect(
        layout.centre,
        [layout.width, layout.height],
        [0.018, 0.038, 0.065, 0.98],
    );
    renderer.queue_text(
        "INCOMING TRANSMISSION",
        [layout.left() + 18.0, BANNER_MARGIN + 12.0],
        1.7,
        [0.24, 0.92, 1.0, 1.0],
    );
    let sender = format!("{} // {}", transmission.sender(), transmission.subject());
    renderer.queue_text(
        &sender,
        [layout.left() + 18.0, BANNER_MARGIN + 36.0],
        1.2,
        [0.68, 0.82, 0.92, 1.0],
    );
    for (index, line) in lines.iter().enumerate() {
        renderer.queue_text(
            line,
            [
                layout.left() + 18.0,
                BANNER_MARGIN + 58.0 + index as f32 * 19.0,
            ],
            BODY_SCALE,
            [0.94, 0.97, 1.0, 1.0],
        );
    }
    let button_colour = if layout.dismiss.contains(cursor) {
        [0.18, 0.48, 0.60, 1.0]
    } else {
        [0.10, 0.29, 0.40, 1.0]
    };
    renderer.queue_rect(layout.dismiss.centre, layout.dismiss.size, button_colour);
    renderer.queue_text(
        "DISMISS",
        [
            layout.dismiss.centre[0] - GuiRenderer::text_width("DISMISS", 1.2) * 0.5,
            layout.dismiss.centre[1] - 4.2,
        ],
        1.2,
        [0.92, 0.98, 1.0, 1.0],
    );
}

pub fn handle_incoming_transmission_click(
    transmission: Option<&Transmission>,
    cursor: [f32; 2],
    viewport: [f32; 2],
) -> bool {
    transmission.is_some_and(|transmission| {
        IncomingLayout::new(transmission, viewport)
            .dismiss
            .contains(cursor)
    })
}

pub fn incoming_transmission_captures_pointer(
    transmission: Option<&Transmission>,
    cursor: [f32; 2],
    viewport: [f32; 2],
) -> bool {
    transmission.is_some_and(|transmission| {
        IncomingLayout::new(transmission, viewport)
            .banner()
            .contains(cursor)
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct IncomingLayout {
    centre: [f32; 2],
    width: f32,
    height: f32,
    dismiss: Rect,
}

impl IncomingLayout {
    fn new(transmission: &Transmission, viewport: [f32; 2]) -> Self {
        let width = (viewport[0] - BANNER_MARGIN * 2.0).clamp(320.0, BANNER_MAX_WIDTH);
        let line_count = wrap_text(transmission.body(), BODY_SCALE, width - 36.0).len();
        let height = 104.0 + line_count as f32 * 19.0;
        let centre = [viewport[0] * 0.5, BANNER_MARGIN + height * 0.5];
        Self {
            centre,
            width,
            height,
            dismiss: Rect {
                centre: [
                    centre[0] + width * 0.5 - DISMISS_BUTTON_SIZE[0] * 0.5 - 12.0,
                    BANNER_MARGIN + height - DISMISS_BUTTON_SIZE[1] * 0.5 - 10.0,
                ],
                size: DISMISS_BUTTON_SIZE,
            },
        }
    }

    fn left(self) -> f32 {
        self.centre[0] - self.width * 0.5
    }

    fn banner(self) -> Rect {
        Rect {
            centre: self.centre,
            size: [self.width, self.height],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    centre: [f32; 2],
    size: [f32; 2],
}

impl Rect {
    fn contains(self, point: [f32; 2]) -> bool {
        let half = [self.size[0] * 0.5, self.size[1] * 0.5];
        (self.centre[0] - half[0]..=self.centre[0] + half[0]).contains(&point[0])
            && (self.centre[1] - half[1]..=self.centre[1] + half[1]).contains(&point[1])
    }
}

pub(super) fn wrap_text(text: &str, scale: f32, maximum_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if !current.is_empty() && GuiRenderer::text_width(&candidate, scale) > maximum_width {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_preserves_all_words() {
        let text = "ONE TWO THREE FOUR FIVE SIX";
        let lines = wrap_text(text, 1.0, GuiRenderer::text_width("ONE TWO", 1.0) + 0.1);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn dismiss_button_and_banner_share_hit_testing_layout() {
        let transmission = Transmission::new("DEEPTEK", "TEST", "A short incoming message.");
        let viewport = [800.0, 600.0];
        let layout = IncomingLayout::new(&transmission, viewport);
        assert!(handle_incoming_transmission_click(
            Some(&transmission),
            layout.dismiss.centre,
            viewport
        ));
        assert!(incoming_transmission_captures_pointer(
            Some(&transmission),
            layout.dismiss.centre,
            viewport
        ));
        assert!(!handle_incoming_transmission_click(
            None,
            layout.dismiss.centre,
            viewport
        ));
    }
}
