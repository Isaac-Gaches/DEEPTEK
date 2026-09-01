use deep_tek::{GuiRenderer, chunks_for_tile_radius};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PauseMenuAction {
    Resume,
    Settings,
    Back,
    SetRenderDistance(RenderDistance),
    SaveAndMainMenu,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RenderDistance {
    #[default]
    Medium,
    High,
}

impl RenderDistance {
    pub(crate) const fn chunk_radii(self) -> (u32, u32) {
        match self {
            Self::Medium => (chunks_for_tile_radius(64), chunks_for_tile_radius(32)),
            Self::High => (chunks_for_tile_radius(96), chunks_for_tile_radius(64)),
        }
    }

    pub(crate) const fn maximum_zoom(self) -> f32 {
        match self {
            Self::Medium => 60.0,
            Self::High => 80.0,
        }
    }
}

#[derive(Default)]
pub(crate) struct PauseMenu {
    error: Option<&'static str>,
    settings_open: bool,
}

impl PauseMenu {
    pub(crate) fn show_settings(&mut self) {
        self.settings_open = true;
    }

    pub(crate) fn show_root(&mut self) {
        self.settings_open = false;
    }

    pub(crate) fn clear_error(&mut self) {
        self.error = None;
        self.settings_open = false;
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
        if self.settings_open {
            if layout.medium.contains(cursor) {
                Some(PauseMenuAction::SetRenderDistance(RenderDistance::Medium))
            } else if layout.high.contains(cursor) {
                Some(PauseMenuAction::SetRenderDistance(RenderDistance::High))
            } else if layout.back.contains(cursor) {
                Some(PauseMenuAction::Back)
            } else {
                None
            }
        } else if layout.resume.contains(cursor) {
            Some(PauseMenuAction::Resume)
        } else if layout.settings.contains(cursor) {
            Some(PauseMenuAction::Settings)
        } else if layout.save_and_menu.contains(cursor) {
            Some(PauseMenuAction::SaveAndMainMenu)
        } else {
            None
        }
    }

    pub(crate) fn queue(
        &self,
        renderer: &mut GuiRenderer,
        viewport: [f32; 2],
        cursor: [f32; 2],
        render_distance: RenderDistance,
    ) {
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
            if self.settings_open {
                "SETTINGS"
            } else {
                "PAUSED"
            },
            layout.panel.centre[0],
            layout.panel.top() + 38.0,
            4.0,
            [0.80, 0.90, 1.0, 1.0],
        );
        if self.settings_open {
            queue_centred_text(
                renderer,
                "RENDER DISTANCE",
                layout.panel.centre[0],
                layout.panel.top() + 92.0,
                2.0,
                [0.65, 0.78, 0.88, 1.0],
            );
            for (button, label, value) in [
                (layout.medium, "MEDIUM", RenderDistance::Medium),
                (layout.high, "HIGH", RenderDistance::High),
            ] {
                queue_button(
                    renderer,
                    button,
                    cursor,
                    label,
                    if value == render_distance {
                        [0.16, 0.55, 0.34, 1.0]
                    } else {
                        [0.10, 0.24, 0.36, 1.0]
                    },
                );
            }
            queue_button(
                renderer,
                layout.back,
                cursor,
                "BACK",
                [0.24, 0.29, 0.36, 1.0],
            );
            return;
        }
        queue_button(
            renderer,
            layout.resume,
            cursor,
            "RESUME",
            [0.12, 0.34, 0.52, 1.0],
        );
        queue_button(
            renderer,
            layout.settings,
            cursor,
            "SETTINGS",
            [0.16, 0.27, 0.42, 1.0],
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
    settings: Rect,
    save_and_menu: Rect,
    medium: Rect,
    high: Rect,
    back: Rect,
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
                centre: [panel.centre[0], panel.centre[1] - 65.0],
                size: button_size,
            },
            settings: Rect {
                centre: [panel.centre[0], panel.centre[1] + 5.0],
                size: button_size,
            },
            save_and_menu: Rect {
                centre: [panel.centre[0], panel.centre[1] + 75.0],
                size: button_size,
            },
            medium: Rect {
                centre: [panel.centre[0] - 82.0, panel.centre[1]],
                size: [140.0, 54.0],
            },
            high: Rect {
                centre: [panel.centre[0] + 82.0, panel.centre[1]],
                size: [140.0, 54.0],
            },
            back: Rect {
                centre: [panel.centre[0], panel.centre[1] + 92.0],
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
        assert_eq!(
            menu.handle_click(layout.settings.centre, viewport),
            Some(PauseMenuAction::Settings)
        );
        assert_eq!(menu.handle_click([0.0, 0.0], viewport), None);
    }

    #[test]
    fn settings_select_render_distance_and_can_go_back() {
        let mut menu = PauseMenu::default();
        let viewport = [800.0, 600.0];
        let layout = PauseLayout::new(viewport);
        menu.show_settings();
        assert_eq!(
            menu.handle_click(layout.high.centre, viewport),
            Some(PauseMenuAction::SetRenderDistance(RenderDistance::High))
        );
        assert_eq!(
            menu.handle_click(layout.back.centre, viewport),
            Some(PauseMenuAction::Back)
        );
    }

    #[test]
    fn render_presets_keep_lighting_work_bounded() {
        assert_eq!(RenderDistance::Medium.chunk_radii(), (2, 1));
        assert_eq!(RenderDistance::High.chunk_radii(), (3, 2));
        assert_eq!(deep_tek::lighting_tile_dimensions(2, 1), [192, 128]);
        assert_eq!(deep_tek::lighting_tile_dimensions(3, 2), [256, 192]);
        assert!(RenderDistance::Medium.maximum_zoom() < RenderDistance::High.maximum_zoom());
    }
}
