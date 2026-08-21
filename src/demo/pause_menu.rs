use deep_tek::GuiRenderer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PauseMenuAction {
    Resume,
    SaveAndMainMenu,
}

#[derive(Default)]
pub(crate) struct PauseMenu {
    error: Option<&'static str>,
}

impl PauseMenu {
    pub(crate) fn clear_error(&mut self) {
        self.error = None;
    }

    pub(crate) fn set_save_failed(&mut self) {
        self.error = Some("SAVE FAILED - WORLD KEPT OPEN");
    }

    pub(crate) fn handle_click(
        &self,
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) -> Option<PauseMenuAction> {
        let layout = PauseLayout::new(viewport);
        if layout.resume.contains(cursor) {
            Some(PauseMenuAction::Resume)
        } else if layout.save_and_menu.contains(cursor) {
            Some(PauseMenuAction::SaveAndMainMenu)
        } else {
            None
        }
    }

    pub(crate) fn queue(&self, renderer: &mut GuiRenderer, viewport: [f32; 2], cursor: [f32; 2]) {
        let layout = PauseLayout::new(viewport);
        renderer.queue_rect(
            [viewport[0] * 0.5, viewport[1] * 0.5],
            viewport,
            [0.0, 0.0, 0.0, 0.58],
        );
        renderer.queue_rect(
            layout.panel.centre,
            layout.panel.size,
            [0.03, 0.05, 0.09, 0.98],
        );
        queue_centred_text(
            renderer,
            "PAUSED",
            layout.panel.centre[0],
            layout.panel.top() + 38.0,
            4.0,
            [0.80, 0.90, 1.0, 1.0],
        );
        queue_button(
            renderer,
            layout.resume,
            cursor,
            "RESUME",
            [0.12, 0.34, 0.52, 1.0],
        );
        queue_button(
            renderer,
            layout.save_and_menu,
            cursor,
            "SAVE AND MAIN MENU",
            [0.10, 0.42, 0.25, 1.0],
        );
        if let Some(error) = self.error {
            queue_centred_text(
                renderer,
                error,
                layout.panel.centre[0],
                layout.panel.bottom() - 42.0,
                1.5,
                [1.0, 0.35, 0.30, 1.0],
            );
        }
    }
}

#[derive(Clone, Copy)]
struct PauseLayout {
    panel: Rect,
    resume: Rect,
    save_and_menu: Rect,
}

impl PauseLayout {
    fn new(viewport: [f32; 2]) -> Self {
        let panel = Rect {
            centre: [viewport[0] * 0.5, viewport[1] * 0.5],
            size: [(viewport[0] - 32.0).clamp(360.0, 560.0), 360.0],
        };
        let button_size = [panel.size[0] - 64.0, 58.0];
        Self {
            panel,
            resume: Rect {
                centre: [panel.centre[0], panel.centre[1] - 35.0],
                size: button_size,
            },
            save_and_menu: Rect {
                centre: [panel.centre[0], panel.centre[1] + 45.0],
                size: button_size,
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

fn queue_button(
    renderer: &mut GuiRenderer,
    button: Rect,
    cursor: [f32; 2],
    label: &str,
    colour: [f32; 4],
) {
    let tint = if button.contains(cursor) {
        [
            colour[0] * 1.25,
            colour[1] * 1.25,
            colour[2] * 1.25,
            colour[3],
        ]
    } else {
        colour
    };
    renderer.queue_rect(button.centre, button.size, tint);
    queue_centred_text(
        renderer,
        label,
        button.centre[0],
        button.centre[1] - 10.5,
        3.0,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_buttons_produce_distinct_actions() {
        let menu = PauseMenu::default();
        let viewport = [800.0, 600.0];
        let layout = PauseLayout::new(viewport);
        assert_eq!(
            menu.handle_click(layout.resume.centre, viewport),
            Some(PauseMenuAction::Resume)
        );
        assert_eq!(
            menu.handle_click(layout.save_and_menu.centre, viewport),
            Some(PauseMenuAction::SaveAndMainMenu)
        );
        assert_eq!(menu.handle_click([0.0, 0.0], viewport), None);
    }
}
